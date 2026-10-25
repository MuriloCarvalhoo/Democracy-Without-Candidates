//! Authentication flows. Login: e-mail + password → face → TOTP (or a recovery code)
//! → session. Password reset: e-mail → face → TOTP + new password. State between
//! steps lives in `auth_challenges` (hash of an HttpOnly cookie token, 5 minutes,
//! single use). Messages are generic and failures are rate limited.

use chrono::Duration;
use turso::Connection;

use super::{
    ClientKeys, Deps, FaceProof, attempt, audit_failure, check_totp, credentials,
    face_not_confirmed, invalid_credentials, invalid_totp, ratelimit, record_totp_step,
    recovery_code_hash, replace_recovery_codes, verify_face, verify_session_factors,
};
use crate::{
    application::{
        actor::Actor,
        audit::{self, Event, Visibility},
        reauth, session,
    },
    domain::identity::{AccountStatus, normalize_email, password_acceptable},
    error::AppError,
    i18n::t,
    infrastructure::{
        clock::{parse_ts, ts},
        crypto::{random_token, sha256_hex},
        db::query_opt,
        password,
    },
};

pub const CHALLENGE_TTL: Duration = Duration::minutes(5);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoginOutcome {
    /// Password accepted; continue with face (same auth token).
    NeedFace,
    /// Face accepted; continue with TOTP or a recovery code.
    NeedTotp,
    /// Session established. `pending` accounts must finish enrollment.
    Session { token: String, pending: bool },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Purpose {
    Login,
    PasswordReset,
}

impl Purpose {
    fn as_db(self) -> &'static str {
        match self {
            Self::Login => "login",
            Self::PasswordReset => "password_reset",
        }
    }
}

fn step_expired() -> AppError {
    AppError::validation("flow", t("auth.error.flow_expired"))
}

pub fn auth_subject(token: &str) -> String {
    format!("auth:{}", sha256_hex(token))
}

struct Challenge {
    token_hash: String,
    user_id: Option<String>,
    keys: ClientKeys,
    password_ok: bool,
    face_ok: bool,
}

async fn new_challenge(
    conn: &Connection,
    deps: &Deps<'_>,
    purpose: Purpose,
    user_id: Option<&str>,
    keys: &ClientKeys,
    password_ok: bool,
) -> Result<String, AppError> {
    let token = random_token(32);
    conn.execute(
        "DELETE FROM auth_challenges WHERE expires_at < ?1",
        [ts(deps.now - Duration::days(1))],
    )
    .await?;
    conn.execute(
        "INSERT INTO auth_challenges (token_hash, purpose, user_id, account_key, password_ok, created_at, expires_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        (
            sha256_hex(&token),
            purpose.as_db(),
            user_id,
            keys.account.as_str(),
            password_ok as i64,
            ts(deps.now),
            ts(deps.now + CHALLENGE_TTL),
        ),
    )
    .await?;
    Ok(token)
}

async fn load_challenge(
    deps: &Deps<'_>,
    token: &str,
    purpose: Purpose,
    ip: Option<String>,
) -> Result<Challenge, AppError> {
    let conn = deps.db.read().await?;
    let row = query_opt(
        &conn,
        "SELECT user_id, account_key, password_ok, face_ok, expires_at FROM auth_challenges
         WHERE token_hash = ?1 AND purpose = ?2 AND consumed_at IS NULL",
        (sha256_hex(token), purpose.as_db()),
    )
    .await?
    .ok_or_else(step_expired)?;
    let expires = parse_ts(&row.get::<String>(4)?).ok_or(AppError::Internal)?;
    if deps.now >= expires {
        return Err(step_expired());
    }
    Ok(Challenge {
        token_hash: sha256_hex(token),
        user_id: row.get(0)?,
        keys: ClientKeys {
            account: row.get(1)?,
            ip,
        },
        password_ok: row.get::<i64>(2)? == 1,
        face_ok: row.get::<i64>(3)? == 1,
    })
}

/// Marks the challenge consumed; fails if another request consumed it first.
async fn consume_challenge(
    conn: &Connection,
    deps: &Deps<'_>,
    token_hash: &str,
) -> Result<(), AppError> {
    let changed = conn
        .execute(
            "UPDATE auth_challenges SET consumed_at = ?2 WHERE token_hash = ?1 AND consumed_at IS NULL",
            (token_hash, ts(deps.now)),
        )
        .await?;
    if changed == 1 {
        Ok(())
    } else {
        Err(step_expired())
    }
}

async fn audit_event(
    conn: &Connection,
    deps: &Deps<'_>,
    kind: &str,
    user_id: &str,
    payload: serde_json::Value,
) -> Result<(), AppError> {
    audit::append(
        conn,
        deps.now,
        Event {
            kind,
            visibility: Visibility::Restricted,
            actor_ref: Some(user_id),
            subject: Some(user_id),
            payload,
        },
    )
    .await?;
    Ok(())
}

async fn start_session(
    conn: &Connection,
    deps: &Deps<'_>,
    user_id: &str,
    keys: &ClientKeys,
    method: &str,
) -> Result<String, AppError> {
    ratelimit::clear_account(conn, keys).await?;
    audit_event(
        conn,
        deps,
        "identity.login_succeeded",
        user_id,
        serde_json::json!({ "method": method }),
    )
    .await?;
    session::create(conn, deps.now, deps.policy, user_id).await
}

struct Account {
    id: String,
    password_hash: String,
    status: AccountStatus,
}

async fn account_by_email(conn: &Connection, email: &str) -> Result<Option<Account>, AppError> {
    let Some(row) = query_opt(
        conn,
        "SELECT id, password_hash, status FROM users WHERE email = ?1",
        [email],
    )
    .await?
    else {
        return Ok(None);
    };
    Ok(Some(Account {
        id: row.get(0)?,
        password_hash: row.get(1)?,
        status: AccountStatus::from_db(&row.get::<String>(2)?).ok_or(AppError::Internal)?,
    }))
}

/// Step 1 of login. Returns the auth token (cookie) and the next step.
pub async fn login_password(
    deps: &Deps<'_>,
    ip: Option<std::net::IpAddr>,
    email: &str,
    password_input: &str,
) -> Result<(Option<String>, LoginOutcome), AppError> {
    let normalized = normalize_email(email).unwrap_or_else(|| email.trim().to_lowercase());
    let keys = ClientKeys::new(&normalized, ip);
    let conn = deps.db.read().await?;
    let account = attempt(deps, &keys, None, async {
        let account = account_by_email(&conn, &normalized).await?;
        let ok = password_input.chars().count() <= 1024
            && password::verify(
                account.as_ref().map(|a| a.password_hash.clone()),
                password_input,
            )
            .await;
        Ok(account
            .filter(|_| ok)
            .ok_or_else(|| ("password", invalid_credentials())))
    })
    .await?;
    if account.status == AccountStatus::Blocked {
        return Err(AppError::Forbidden);
    }
    let face_enrolled = credentials(&conn, &account.id)
        .await?
        .face_template
        .is_some();
    let tx = deps.db.write().await?;
    let result = if face_enrolled {
        let token =
            new_challenge(&tx, deps, Purpose::Login, Some(&account.id), &keys, true).await?;
        (Some(token), LoginOutcome::NeedFace)
    } else {
        // Pending account without face: session only to finish enrollment.
        let token = start_session(&tx, deps, &account.id, &keys, "password_pending").await?;
        (
            None,
            LoginOutcome::Session {
                token,
                pending: true,
            },
        )
    };
    tx.commit().await?;
    Ok(result)
}

/// Face step for login or password reset.
pub async fn auth_face(
    deps: &Deps<'_>,
    ip: Option<std::net::IpAddr>,
    token: &str,
    purpose: Purpose,
    proof: FaceProof<'_>,
) -> Result<LoginOutcome, AppError> {
    let ip = ClientKeys::new("", ip).ip;
    let c = load_challenge(deps, token, purpose, ip).await?;
    if purpose == Purpose::Login && !c.password_ok {
        return Err(step_expired());
    }
    attempt(deps, &c.keys, c.user_id.as_deref(), async {
        let ok = verify_face(deps, &auth_subject(token), c.user_id.as_deref(), &proof).await?;
        Ok(ok
            .then_some(())
            .ok_or_else(|| ("face", face_not_confirmed())))
    })
    .await?;
    let user_id = c.user_id.as_deref().ok_or(AppError::Internal)?;
    let conn = deps.db.read().await?;
    let totp_confirmed = credentials(&conn, user_id).await?.totp_secret.is_some();
    let tx = deps.db.write().await?;
    let outcome = if purpose == Purpose::Login && !totp_confirmed {
        consume_challenge(&tx, deps, &c.token_hash).await?;
        let token = start_session(&tx, deps, user_id, &c.keys, "password_face_pending").await?;
        LoginOutcome::Session {
            token,
            pending: true,
        }
    } else {
        tx.execute(
            "UPDATE auth_challenges SET face_ok = 1 WHERE token_hash = ?1 AND consumed_at IS NULL",
            [c.token_hash.as_str()],
        )
        .await?;
        LoginOutcome::NeedTotp
    };
    tx.commit().await?;
    Ok(outcome)
}

async fn ready_for_second_factor(
    deps: &Deps<'_>,
    token: &str,
    purpose: Purpose,
    ip: Option<std::net::IpAddr>,
) -> Result<(Challenge, String), AppError> {
    let c = load_challenge(deps, token, purpose, ClientKeys::new("", ip).ip).await?;
    let ready = c.face_ok && (purpose == Purpose::PasswordReset || c.password_ok);
    match (&c.user_id, ready) {
        (Some(user_id), true) => {
            let user_id = user_id.clone();
            Ok((c, user_id))
        }
        _ => Err(step_expired()),
    }
}

/// One counted TOTP attempt; parallel guesses within a challenge share the limit.
async fn totp_attempt(
    deps: &Deps<'_>,
    c: &Challenge,
    user_id: &str,
    code: &str,
) -> Result<i64, AppError> {
    attempt(deps, &c.keys, Some(user_id), async {
        Ok(check_totp(deps, user_id, code)
            .await?
            .ok_or_else(|| ("totp", invalid_totp())))
    })
    .await
}

/// Final login step with TOTP.
pub async fn login_totp(
    deps: &Deps<'_>,
    ip: Option<std::net::IpAddr>,
    token: &str,
    code: &str,
) -> Result<String, AppError> {
    let (c, user_id) = ready_for_second_factor(deps, token, Purpose::Login, ip).await?;
    let step = totp_attempt(deps, &c, &user_id, code).await?;
    let tx = deps.db.write().await?;
    record_totp_step(&tx, &user_id, step).await?;
    consume_challenge(&tx, deps, &c.token_hash).await?;
    let session = start_session(&tx, deps, &user_id, &c.keys, "totp").await?;
    tx.commit().await?;
    Ok(session)
}

/// Consumes one recovery code atomically (at most one concurrent use succeeds).
async fn consume_recovery_code(
    conn: &Connection,
    deps: &Deps<'_>,
    user_id: &str,
    code: &str,
) -> Result<bool, AppError> {
    let Some(hash) = recovery_code_hash(code) else {
        return Ok(false);
    };
    let changed = conn
        .execute(
            "UPDATE recovery_codes SET used_at = ?3
             WHERE code_hash = ?1 AND user_id = ?2 AND used_at IS NULL AND revoked_at IS NULL",
            (hash, user_id, ts(deps.now)),
        )
        .await?;
    Ok(changed == 1)
}

/// Final login step with a recovery code instead of TOTP. Password and face were
/// already verified; TOTP must be re-enrolled before critical actions.
pub async fn login_recovery_code(
    deps: &Deps<'_>,
    ip: Option<std::net::IpAddr>,
    token: &str,
    code: &str,
) -> Result<String, AppError> {
    let (c, user_id) = ready_for_second_factor(deps, token, Purpose::Login, ip).await?;
    let reserved = ratelimit::reserve(deps.db, deps.now, &c.keys).await?;
    let tx = deps.db.write().await?;
    if !consume_recovery_code(&tx, deps, &user_id, code).await? {
        tx.rollback().await?;
        audit_failure(deps, Some(&user_id), "recovery_code").await?;
        return Err(AppError::validation(
            "recovery",
            t("auth.error.recovery_code"),
        ));
    }
    ratelimit::release(&tx, reserved).await?;
    consume_challenge(&tx, deps, &c.token_hash).await?;
    tx.execute(
        "UPDATE user_credentials SET totp_reenroll_required = 1, totp_pending_secret = NULL, updated_at = ?2 WHERE user_id = ?1",
        (user_id.as_str(), ts(deps.now)),
    )
    .await?;
    audit_event(
        &tx,
        deps,
        "identity.recovery_code_used",
        &user_id,
        serde_json::json!({}),
    )
    .await?;
    let session = start_session(&tx, deps, &user_id, &c.keys, "recovery_code").await?;
    tx.commit().await?;
    Ok(session)
}

/// Step 1 of password reset (no e-mail is sent). Unknown e-mails get a token too,
/// whose face step always fails, so the response does not reveal accounts.
pub async fn reset_start(
    deps: &Deps<'_>,
    ip: Option<std::net::IpAddr>,
    email: &str,
) -> Result<String, AppError> {
    let normalized = normalize_email(email)
        .ok_or_else(|| AppError::validation("email", t("auth.email.invalid")))?;
    let keys = ClientKeys::new(&normalized, ip);
    let conn = deps.db.read().await?;
    ratelimit::check(&conn, deps.now, &keys).await?;
    let user_id = match account_by_email(&conn, &normalized).await? {
        Some(a) if a.status == AccountStatus::Active => Some(a.id),
        _ => None,
    };
    let tx = deps.db.write().await?;
    let token = new_challenge(
        &tx,
        deps,
        Purpose::PasswordReset,
        user_id.as_deref(),
        &keys,
        false,
    )
    .await?;
    tx.commit().await?;
    Ok(token)
}

/// Final reset step: TOTP + new password. Success revokes every session; any
/// failure keeps the previous hash. Address and territory are never touched.
pub async fn reset_finish(
    deps: &Deps<'_>,
    ip: Option<std::net::IpAddr>,
    token: &str,
    code: &str,
    new_password: &str,
) -> Result<(), AppError> {
    if !password_acceptable(new_password) {
        return Err(AppError::validation("password", t("auth.password.policy")));
    }
    let (c, user_id) = ready_for_second_factor(deps, token, Purpose::PasswordReset, ip).await?;
    let step = totp_attempt(deps, &c, &user_id, code).await?;
    let hash = password::hash(new_password).await?;
    let tx = deps.db.write().await?;
    record_totp_step(&tx, &user_id, step).await?;
    consume_challenge(&tx, deps, &c.token_hash).await?;
    tx.execute(
        "UPDATE users SET password_hash = ?2, updated_at = ?3 WHERE id = ?1",
        (user_id.as_str(), hash, ts(deps.now)),
    )
    .await?;
    session::revoke_all(&tx, deps.now, &user_id, "password_reset").await?;
    ratelimit::clear_account(&tx, &c.keys).await?;
    audit_event(
        &tx,
        deps,
        "identity.password_reset",
        &user_id,
        serde_json::json!({}),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

/// Signed-in password change: current password + face + TOTP. Revokes all sessions.
pub async fn change_password(
    deps: &Deps<'_>,
    actor: &Actor,
    current: &str,
    new_password: &str,
    proof: FaceProof<'_>,
    code: &str,
) -> Result<(), AppError> {
    if !password_acceptable(new_password) {
        return Err(AppError::validation("password", t("auth.password.policy")));
    }
    let step = verify_session_factors(deps, &actor.user_id, Some(current), &proof, code).await?;
    let hash = password::hash(new_password).await?;
    let tx = deps.db.write().await?;
    record_totp_step(&tx, &actor.user_id, step).await?;
    tx.execute(
        "UPDATE users SET password_hash = ?2, updated_at = ?3 WHERE id = ?1",
        (actor.user_id.as_str(), hash, ts(deps.now)),
    )
    .await?;
    session::revoke_all(&tx, deps.now, &actor.user_id, "password_changed").await?;
    audit_event(
        &tx,
        deps,
        "identity.password_changed",
        &actor.user_id,
        serde_json::json!({}),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

/// New set of ten recovery codes (password + face + TOTP); previous codes are revoked.
pub async fn regenerate_recovery_codes(
    deps: &Deps<'_>,
    actor: &Actor,
    password_input: &str,
    proof: FaceProof<'_>,
    code: &str,
) -> Result<Vec<String>, AppError> {
    if actor.status != AccountStatus::Active {
        return Err(AppError::Forbidden);
    }
    let step =
        verify_session_factors(deps, &actor.user_id, Some(password_input), &proof, code).await?;
    let tx = deps.db.write().await?;
    record_totp_step(&tx, &actor.user_id, step).await?;
    let codes = replace_recovery_codes(&tx, deps.now, &actor.user_id).await?;
    audit_event(
        &tx,
        deps,
        "identity.recovery_codes_regenerated",
        &actor.user_id,
        serde_json::json!({}),
    )
    .await?;
    tx.commit().await?;
    Ok(codes)
}

/// Face + TOTP for the current session; records a grant bound to action + resource.
pub async fn reauthenticate(
    deps: &Deps<'_>,
    actor: &Actor,
    action: &str,
    resource: &str,
    proof: FaceProof<'_>,
    code: &str,
) -> Result<String, AppError> {
    let valid_action = action.len() <= 64
        && action.split_once(':').is_some_and(|(a, b)| {
            !a.is_empty()
                && !b.is_empty()
                && (a.chars().chain(b.chars())).all(|c| c.is_ascii_lowercase() || c == '_')
        });
    if !valid_action || resource.is_empty() || resource.len() > 128 {
        return Err(AppError::validation(
            "action",
            t("auth.reauth.invalid_action"),
        ));
    }
    if actor.status != AccountStatus::Active {
        return Err(AppError::Forbidden);
    }
    let step = verify_session_factors(deps, &actor.user_id, None, &proof, code).await?;
    let tx = deps.db.write().await?;
    record_totp_step(&tx, &actor.user_id, step).await?;
    let id = reauth::grant(&tx, deps.now, &actor.session_hash, action, resource).await?;
    // Action only: resource ids could link a reauthentication to a ballot.
    audit_event(
        &tx,
        deps,
        "identity.reauth_granted",
        &actor.user_id,
        serde_json::json!({ "action": action }),
    )
    .await?;
    tx.commit().await?;
    Ok(id)
}

/// Revokes the current session (logout).
pub async fn logout(deps: &Deps<'_>, session_hash: &str) -> Result<(), AppError> {
    let tx = deps.db.write().await?;
    session::revoke(&tx, deps.now, session_hash, "logout").await?;
    tx.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        super::{
            enroll::tests::{actor_for, enrolled, face_proof_challenge, wrong_code},
            issue_face_challenge,
            testing::*,
        },
        *,
    };
    use crate::infrastructure::totp;

    async fn face_step(
        w: &World,
        deps: &Deps<'_>,
        token: &str,
        purpose: Purpose,
        image: &[u8],
    ) -> Result<LoginOutcome, AppError> {
        let c = issue_face_challenge(&w.t.db, deps.now, &auth_subject(token))
            .await
            .unwrap();
        auth_face(
            deps,
            None,
            token,
            purpose,
            FaceProof {
                challenge: &c,
                image,
            },
        )
        .await
    }

    async fn count(w: &World, sql: &str) -> i64 {
        let conn = w.t.db.read().await.unwrap();
        query_opt(&conn, sql, ())
            .await
            .unwrap()
            .unwrap()
            .get(0)
            .unwrap()
    }

    #[tokio::test]
    async fn login_requires_password_face_and_totp_with_replay_blocked() {
        let w = World::new().await;
        let e = enrolled(&w, "ana@example.org").await;
        let d = w.deps();
        // Wrong password and unknown e-mail give the same generic error.
        assert_eq!(
            login_password(&d, None, &e.email, "senha errada qualquer")
                .await
                .err(),
            Some(invalid_credentials())
        );
        assert_eq!(
            login_password(&d, None, "ninguem@example.org", PASSWORD)
                .await
                .err(),
            Some(invalid_credentials())
        );

        let (token, next) = login_password(&d, None, "ANA@example.org", PASSWORD)
            .await
            .unwrap();
        assert_eq!(next, LoginOutcome::NeedFace);
        let token = token.unwrap();
        // TOTP before face is refused.
        let code = totp::code_at(&e.totp_secret, w.now.timestamp());
        assert!(login_totp(&d, None, &token, &code).await.is_err());
        assert_eq!(
            face_step(&w, &d, &token, Purpose::Login, b"outro rosto")
                .await
                .err(),
            Some(face_not_confirmed())
        );
        assert_eq!(
            face_step(&w, &d, &token, Purpose::Login, FACE)
                .await
                .unwrap(),
            LoginOutcome::NeedTotp
        );
        assert_eq!(
            login_totp(&d, None, &token, &wrong_code(&code)).await.err(),
            Some(invalid_totp())
        );
        let s = login_totp(&d, None, &token, &code).await.unwrap();
        let actor = actor_for(&w, &s).await;
        assert_eq!(actor.user_id, e.user_id);
        // Challenge is single use; same TOTP step cannot be replayed in a new login.
        assert!(login_totp(&d, None, &token, &code).await.is_err());
        let (t2, _) = login_password(&d, None, &e.email, PASSWORD).await.unwrap();
        let t2 = t2.unwrap();
        face_step(&w, &d, &t2, Purpose::Login, FACE).await.unwrap();
        assert_eq!(
            login_totp(&d, None, &t2, &code).await.err(),
            Some(invalid_totp()),
            "replay"
        );
        // Challenge expires.
        let late = w.at(w.now + CHALLENGE_TTL);
        let next_code = totp::code_at(&e.totp_secret, late.now.timestamp());
        assert_eq!(
            login_totp(&late, None, &t2, &next_code).await.err(),
            Some(step_expired())
        );
        assert!(count(&w, "SELECT COUNT(*) FROM audit_events WHERE kind = 'identity.auth_failed' AND payload LIKE '%@%'").await == 0);
    }

    #[tokio::test]
    async fn account_rate_limit_after_five_failures_then_window_passes() {
        let w = World::new().await;
        let e = enrolled(&w, "ana@example.org").await;
        let d = w.deps();
        for _ in 0..5 {
            assert_eq!(
                login_password(&d, None, &e.email, "senha errada qualquer")
                    .await
                    .err(),
                Some(invalid_credentials())
            );
        }
        assert_eq!(
            login_password(&d, None, &e.email, PASSWORD).await.err(),
            Some(AppError::RateLimited)
        );
        let later = w.at(w.now + ratelimit::WINDOW + Duration::seconds(1));
        assert!(
            login_password(&later, None, &e.email, PASSWORD)
                .await
                .is_ok()
        );
    }

    #[tokio::test]
    async fn recovery_code_login_is_single_use_even_concurrently_and_forces_reenroll() {
        let w = World::new().await;
        let e = enrolled(&w, "ana@example.org").await;
        let d = w.deps();
        let mut tokens = Vec::new();
        for _ in 0..2 {
            let (t, _) = login_password(&d, None, &e.email, PASSWORD).await.unwrap();
            let t = t.unwrap();
            face_step(&w, &d, &t, Purpose::Login, FACE).await.unwrap();
            tokens.push(t);
        }
        let code = e.recovery_codes[0].clone();
        let (a, b) = tokio::join!(
            login_recovery_code(&d, None, &tokens[0], &code),
            login_recovery_code(&d, None, &tokens[1], &code)
        );
        assert_eq!(
            [a.is_ok(), b.is_ok()].iter().filter(|ok| **ok).count(),
            1,
            "exactly one use"
        );
        let session = a.or(b).unwrap();
        let actor = actor_for(&w, &session).await;
        // Critical actions blocked until TOTP is re-enrolled.
        let c = face_proof_challenge(&w, &e.user_id).await;
        let totp_code = totp::code_at(&e.totp_secret, w.now.timestamp());
        assert!(matches!(
            reauthenticate(
                &d,
                &actor,
                "profile:privacy",
                &e.user_id,
                FaceProof {
                    challenge: &c,
                    image: FACE
                },
                &totp_code
            )
            .await,
            Err(AppError::Conflict(_))
        ));
        let prov = super::super::enroll::begin_totp(&d, &actor).await.unwrap();
        let new_secret = totp_rs::Secret::try_from_base32(&prov.secret_base32)
            .unwrap()
            .as_bytes()
            .to_vec();
        let done = super::super::enroll::confirm_totp(
            &d,
            &actor,
            &totp::code_at(&new_secret, w.now.timestamp()),
        )
        .await
        .unwrap();
        assert!(done.recovery_codes.is_none());
        assert_eq!(
            count(
                &w,
                "SELECT COUNT(*) FROM audit_events WHERE kind = 'identity.recovery_code_used'"
            )
            .await,
            1
        );
        let later = w.at(w.now + Duration::seconds(30));
        let c = issue_face_challenge(&w.t.db, later.now, &super::super::user_subject(&e.user_id))
            .await
            .unwrap();
        reauthenticate(
            &later,
            &actor,
            "profile:privacy",
            &e.user_id,
            FaceProof {
                challenge: &c,
                image: FACE,
            },
            &totp::code_at(&new_secret, later.now.timestamp()),
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn password_reset_needs_face_and_totp_and_failures_keep_old_hash() {
        let w = World::new().await;
        let e = enrolled(&w, "ana@example.org").await;
        let d = w.deps();
        let old_hash = || async { count_str(&w, "SELECT password_hash FROM users").await };
        let before = old_hash().await;
        // Unknown e-mail: token issued, face never matches.
        let ghost = reset_start(&d, None, "ninguem@example.org").await.unwrap();
        assert_eq!(
            face_step(&w, &d, &ghost, Purpose::PasswordReset, FACE)
                .await
                .err(),
            Some(face_not_confirmed())
        );

        let t = reset_start(&d, None, &e.email).await.unwrap();
        assert!(
            reset_finish(&d, None, &t, "123456", "nova senha bem longa")
                .await
                .is_err(),
            "face first"
        );
        face_step(&w, &d, &t, Purpose::PasswordReset, FACE)
            .await
            .unwrap();
        let code = totp::code_at(&e.totp_secret, w.now.timestamp());
        assert!(
            reset_finish(&d, None, &t, &wrong_code(&code), "nova senha bem longa")
                .await
                .is_err()
        );
        assert!(reset_finish(&d, None, &t, &code, "curta").await.is_err());
        assert_eq!(old_hash().await, before);
        reset_finish(&d, None, &t, &code, "nova senha bem longa")
            .await
            .unwrap();
        assert_ne!(old_hash().await, before);
        // All sessions revoked; old password no longer works, new one does.
        assert!(
            session::resolve(&w.t.db, w.now, d.policy, &e.session_token, false)
                .await
                .is_err()
        );
        assert!(login_password(&d, None, &e.email, PASSWORD).await.is_err());
        assert!(
            login_password(&d, None, &e.email, "nova senha bem longa")
                .await
                .is_ok()
        );
        assert_eq!(count(&w, "SELECT COUNT(*) FROM addresses").await, 1);
    }

    async fn count_str(w: &World, sql: &str) -> String {
        let conn = w.t.db.read().await.unwrap();
        query_opt(&conn, sql, ())
            .await
            .unwrap()
            .unwrap()
            .get(0)
            .unwrap()
    }

    #[tokio::test]
    async fn change_password_and_regenerate_codes_require_all_factors() {
        let w = World::new().await;
        let e = enrolled(&w, "ana@example.org").await;
        let d = w.deps();
        let actor = actor_for(&w, &e.session_token).await;
        let code = totp::code_at(&e.totp_secret, w.now.timestamp());
        let c = face_proof_challenge(&w, &e.user_id).await;
        assert_eq!(
            regenerate_recovery_codes(
                &d,
                &actor,
                "senha errada qualquer",
                FaceProof {
                    challenge: &c,
                    image: FACE
                },
                &code
            )
            .await
            .err(),
            Some(invalid_credentials())
        );
        let c = face_proof_challenge(&w, &e.user_id).await;
        let codes = regenerate_recovery_codes(
            &d,
            &actor,
            PASSWORD,
            FaceProof {
                challenge: &c,
                image: FACE,
            },
            &code,
        )
        .await
        .unwrap();
        assert_eq!(codes.len(), 10);
        assert_eq!(
            count(
                &w,
                "SELECT COUNT(*) FROM recovery_codes WHERE revoked_at IS NOT NULL"
            )
            .await,
            10
        );
        // Old codes no longer work.
        let tx = w.t.db.write().await.unwrap();
        assert!(
            !consume_recovery_code(&tx, &d, &e.user_id, &e.recovery_codes[1])
                .await
                .unwrap()
        );
        drop(tx);

        let later = w.at(w.now + Duration::seconds(30));
        let code2 = totp::code_at(&e.totp_secret, later.now.timestamp());
        let c = issue_face_challenge(&w.t.db, later.now, &super::super::user_subject(&e.user_id))
            .await
            .unwrap();
        change_password(
            &later,
            &actor,
            PASSWORD,
            "outra senha bem longa",
            FaceProof {
                challenge: &c,
                image: FACE,
            },
            &code2,
        )
        .await
        .unwrap();
        assert!(
            session::resolve(&w.t.db, later.now, d.policy, &e.session_token, false)
                .await
                .is_err()
        );
        assert_eq!(
            count(
                &w,
                "SELECT COUNT(*) FROM audit_events WHERE kind = 'identity.password_changed'"
            )
            .await,
            1
        );
    }

    #[tokio::test]
    async fn reauth_grant_bound_to_action_and_session() {
        let w = World::new().await;
        let e = enrolled(&w, "ana@example.org").await;
        let d = w.deps();
        let actor = actor_for(&w, &e.session_token).await;
        let code = totp::code_at(&e.totp_secret, w.now.timestamp());
        let c = face_proof_challenge(&w, &e.user_id).await;
        assert!(
            reauthenticate(
                &d,
                &actor,
                "bad action",
                "r",
                FaceProof {
                    challenge: &c,
                    image: FACE
                },
                &code
            )
            .await
            .is_err()
        );
        let c = face_proof_challenge(&w, &e.user_id).await;
        reauthenticate(
            &d,
            &actor,
            "proposal:sign",
            "p1",
            FaceProof {
                challenge: &c,
                image: FACE,
            },
            &code,
        )
        .await
        .unwrap();
        let tx = w.t.db.write().await.unwrap();
        assert!(
            reauth::consume(&tx, w.now, &actor.session_hash, "proposal:vote", "p1")
                .await
                .is_err()
        );
        reauth::consume(&tx, w.now, &actor.session_hash, "proposal:sign", "p1")
            .await
            .unwrap();
        tx.commit().await.unwrap();
        // Replayed face challenge is rejected.
        assert!(
            reauthenticate(
                &d,
                &actor,
                "proposal:sign",
                "p1",
                FaceProof {
                    challenge: &c,
                    image: FACE
                },
                &code
            )
            .await
            .is_err()
        );
        let payload = count_str(
            &w,
            "SELECT payload FROM audit_events WHERE kind = 'identity.reauth_granted'",
        )
        .await;
        assert_eq!(payload, r#"{"action":"proposal:sign"}"#);
    }

    async fn reauth_with(
        w: &World,
        d: &Deps<'_>,
        actor: &Actor,
        code: &str,
    ) -> Result<String, AppError> {
        let c = issue_face_challenge(&w.t.db, d.now, &super::super::user_subject(&actor.user_id))
            .await
            .unwrap();
        reauthenticate(
            d,
            actor,
            "proposal:vote",
            "p1",
            FaceProof {
                challenge: &c,
                image: FACE,
            },
            code,
        )
        .await
    }

    #[tokio::test]
    async fn anonymous_login_failures_never_block_session_reauth() {
        let w = World::new().await;
        let e = enrolled(&w, "ana@example.org").await;
        let d = w.deps();
        let actor = actor_for(&w, &e.session_token).await;
        for _ in 0..ratelimit::ACCOUNT_LIMIT {
            assert_eq!(
                login_password(&d, None, &e.email, "senha errada qualquer")
                    .await
                    .err(),
                Some(invalid_credentials())
            );
        }
        assert_eq!(
            login_password(&d, None, &e.email, PASSWORD).await.err(),
            Some(AppError::RateLimited)
        );
        let code = totp::code_at(&e.totp_secret, w.now.timestamp());
        reauth_with(&w, &d, &actor, &code).await.unwrap();

        // Failures of the signed-in session itself are still limited.
        for _ in 0..ratelimit::ACCOUNT_LIMIT {
            assert_eq!(
                reauth_with(&w, &d, &actor, &wrong_code(&code)).await.err(),
                Some(invalid_totp())
            );
        }
        let later = w.at(w.now + Duration::seconds(30));
        let next = totp::code_at(&e.totp_secret, later.now.timestamp());
        assert_eq!(
            reauth_with(&w, &later, &actor, &next).await.err(),
            Some(AppError::RateLimited)
        );
    }

    /// Runs `n` copies of `call` concurrently on worker threads.
    async fn parallel<F, Fut>(n: usize, call: F) -> Vec<Result<(), AppError>>
    where
        F: Fn() -> Fut,
        Fut: Future<Output = Result<(), AppError>> + Send + 'static,
    {
        let tasks: Vec<_> = (0..n).map(|_| tokio::spawn(call())).collect();
        let mut out = Vec::new();
        for t in tasks {
            out.push(t.await.unwrap());
        }
        out
    }

    fn task_deps(
        db: &crate::infrastructure::db::Db,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Deps<'_> {
        static FACE_VERIFIER: crate::infrastructure::face::FaceVerifier =
            crate::infrastructure::face::FaceVerifier::Synthetic;
        static SECRETS: std::sync::LazyLock<crate::infrastructure::secrets::SecretBox> =
            std::sync::LazyLock::new(|| {
                crate::infrastructure::secrets::SecretBox::from_key(&[9; 32])
            });
        Deps {
            db,
            now,
            face: &FACE_VERIFIER,
            secrets: &SECRETS,
            policy: session::SessionPolicy::from_secs(900, 8 * 3600),
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn parallel_wrong_attempts_never_exceed_the_limit() {
        let w = World::new().await;
        let e = enrolled(&w, "ana@example.org").await;
        let limit = ratelimit::ACCOUNT_LIMIT as usize;
        let count_errors = |results: &[Result<(), AppError>], expected: AppError| {
            let rejected = results
                .iter()
                .filter(|r| r.as_ref().err() == Some(&expected))
                .count();
            let limited = results
                .iter()
                .filter(|r| r.as_ref().err() == Some(&AppError::RateLimited))
                .count();
            (rejected, limited)
        };

        let results = parallel(3 * limit, || {
            let (db, email, now) = (w.t.db.clone(), e.email.clone(), w.now);
            async move {
                login_password(&task_deps(&db, now), None, &email, "senha errada qualquer")
                    .await
                    .map(|_| ())
            }
        })
        .await;
        assert_eq!(
            count_errors(&results, invalid_credentials()),
            (limit, 2 * limit)
        );

        // Parallel TOTP guesses within one challenge share the same limit.
        let later = w.at(w.now + ratelimit::WINDOW + Duration::seconds(1));
        let (token, _) = login_password(&later, None, &e.email, PASSWORD)
            .await
            .unwrap();
        let token = token.unwrap();
        face_step(&w, &later, &token, Purpose::Login, FACE)
            .await
            .unwrap();
        let wrong = wrong_code(&totp::code_at(&e.totp_secret, later.now.timestamp()));
        let results = parallel(3 * limit, || {
            let (db, token, wrong, now) = (w.t.db.clone(), token.clone(), wrong.clone(), later.now);
            async move {
                login_totp(&task_deps(&db, now), None, &token, &wrong)
                    .await
                    .map(|_| ())
            }
        })
        .await;
        assert_eq!(count_errors(&results, invalid_totp()), (limit, 2 * limit));
        assert_eq!(
            count(
                &w,
                "SELECT COUNT(*) FROM audit_events WHERE kind = 'identity.auth_failed'"
            )
            .await,
            2 * limit as i64
        );
    }
}
