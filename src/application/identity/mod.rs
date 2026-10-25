//! Identity use cases: registration, enrollment (face + TOTP + recovery codes),
//! multi-step login, password/TOTP recovery, reauthentication and profile.
//! Factors are verified here, on the server; the browser never declares success.

pub mod auth;
pub mod enroll;
pub mod profile;
pub mod ratelimit;
pub mod register;

use chrono::{DateTime, Duration, Utc};
use turso::Connection;

use super::{
    audit::{self, Event, Visibility},
    session::SessionPolicy,
};
use crate::{
    error::AppError,
    i18n::t,
    infrastructure::{
        clock::ts,
        crypto::{random_token, sha256_hex},
        db::{Db, query_opt},
        face::{FaceOutcome, FaceSample, FaceTemplate, FaceVerifier},
        secrets::SecretBox,
        totp,
    },
};

pub use ratelimit::ClientKeys;

/// Server dependencies of the identity use cases (explicit for tests).
pub struct Deps<'a> {
    pub db: &'a Db,
    pub now: DateTime<Utc>,
    pub face: &'a FaceVerifier,
    pub secrets: &'a SecretBox,
    pub policy: SessionPolicy,
    /// Development/test only (`DWC_DEV_SKIP_TOTP`): TOTP codes are not checked.
    pub skip_totp: bool,
}

/// One face capture bound to a single-use server challenge.
pub struct FaceProof<'a> {
    pub challenge: &'a str,
    pub image: &'a [u8],
}

pub const FACE_CHALLENGE_TTL: Duration = Duration::minutes(5);
/// Largest accepted capture (decoded bytes).
pub const FACE_IMAGE_MAX: usize = 1_500_000;

/// Generic credential failure: never reveals which account or factor exists.
pub fn invalid_credentials() -> AppError {
    AppError::validation("credentials", t("auth.error.invalid"))
}

pub fn face_not_confirmed() -> AppError {
    AppError::validation("face", t("auth.error.face"))
}

pub fn invalid_totp() -> AppError {
    AppError::validation("totp", t("auth.error.totp"))
}

pub fn user_subject(user_id: &str) -> String {
    format!("user:{user_id}")
}

/// Issues a face challenge for `subject` (a user or an authentication flow).
#[tracing::instrument(level = "debug", skip_all, err(level = "debug"))]
pub async fn issue_face_challenge(
    db: &Db,
    now: DateTime<Utc>,
    subject: &str,
) -> Result<String, AppError> {
    let challenge = random_token(16);
    let tx = db.write().await?;
    tx.execute(
        "DELETE FROM face_challenges WHERE expires_at < ?1",
        [ts(now - Duration::days(1))],
    )
    .await?;
    tx.execute(
        "INSERT INTO face_challenges (challenge_hash, subject, created_at, expires_at) VALUES (?1, ?2, ?3, ?4)",
        (sha256_hex(&challenge), subject, ts(now), ts(now + FACE_CHALLENGE_TTL)),
    )
    .await?;
    tx.commit().await?;
    Ok(challenge)
}

/// Consumes the challenge once inside the caller's transaction.
async fn consume_face_challenge_in(
    conn: &Connection,
    now: DateTime<Utc>,
    subject: &str,
    challenge: &str,
) -> Result<(), AppError> {
    let changed = conn
        .execute(
            "UPDATE face_challenges SET consumed_at = ?3
             WHERE challenge_hash = ?1 AND subject = ?2 AND consumed_at IS NULL AND expires_at > ?3",
            (sha256_hex(challenge), subject, ts(now)),
        )
        .await?;
    if changed == 1 {
        Ok(())
    } else {
        Err(AppError::validation("face", t("auth.error.face_challenge")))
    }
}

/// Consumes the challenge once (its own transaction, committed before the engine call).
async fn consume_face_challenge(
    db: &Db,
    now: DateTime<Utc>,
    subject: &str,
    challenge: &str,
) -> Result<(), AppError> {
    let tx = db.write().await?;
    consume_face_challenge_in(&tx, now, subject, challenge).await?;
    tx.commit().await?;
    Ok(())
}

struct Credentials {
    face_template: Option<Vec<u8>>,
    totp_secret: Option<Vec<u8>>,
    totp_last_step: Option<i64>,
    totp_pending_secret: Option<Vec<u8>>,
    totp_reenroll_required: bool,
}

async fn credentials(conn: &Connection, user_id: &str) -> Result<Credentials, AppError> {
    let row = query_opt(
        conn,
        "SELECT face_template, totp_secret, totp_last_step, totp_pending_secret, totp_reenroll_required
         FROM user_credentials WHERE user_id = ?1",
        [user_id],
    )
    .await?;
    Ok(match row {
        Some(r) => Credentials {
            face_template: r.get(0)?,
            totp_secret: r.get(1)?,
            totp_last_step: r.get(2)?,
            totp_pending_secret: r.get(3)?,
            totp_reenroll_required: r.get::<i64>(4)? == 1,
        },
        None => Credentials {
            face_template: None,
            totp_secret: None,
            totp_last_step: None,
            totp_pending_secret: None,
            totp_reenroll_required: false,
        },
    })
}

fn face_aad(user_id: &str) -> String {
    format!("face:{user_id}")
}

fn totp_aad(user_id: &str) -> String {
    format!("totp:{user_id}")
}

/// Consumes the challenge and compares the capture with the enrolled template.
async fn verify_face(
    deps: &Deps<'_>,
    subject: &str,
    user_id: Option<&str>,
    proof: &FaceProof<'_>,
) -> Result<bool, AppError> {
    consume_face_challenge(deps.db, deps.now, subject, proof.challenge).await?;
    match_face(deps, user_id, proof).await
}

/// Compares a capture (challenge already consumed) with the enrolled template.
async fn match_face(
    deps: &Deps<'_>,
    user_id: Option<&str>,
    proof: &FaceProof<'_>,
) -> Result<bool, AppError> {
    let Some(user_id) = user_id else {
        return Ok(false);
    };
    let conn = deps.db.read().await?;
    let Some(sealed) = credentials(&conn, user_id).await?.face_template else {
        return Ok(false);
    };
    let template = FaceTemplate(deps.secrets.open(&sealed, &face_aad(user_id))?);
    let outcome = deps
        .face
        .verify(
            &template,
            FaceSample {
                image: proof.image,
                challenge: proof.challenge,
            },
        )
        .await?;
    Ok(outcome == FaceOutcome::Match)
}

/// Step accepted for the confirmed TOTP secret (not yet recorded).
async fn check_totp(deps: &Deps<'_>, user_id: &str, code: &str) -> Result<Option<i64>, AppError> {
    let conn = deps.db.read().await?;
    let creds = credentials(&conn, user_id).await?;
    let Some(sealed) = creds.totp_secret else {
        return Ok(None);
    };
    if deps.skip_totp {
        return Ok(Some(SKIPPED_TOTP_STEP));
    }
    let secret = deps.secrets.open(&sealed, &totp_aad(user_id))?;
    Ok(totp::verify(
        &secret,
        code,
        deps.now.timestamp(),
        creds.totp_last_step,
    ))
}

/// Step returned by `check_totp` when `skip_totp` is on; never recorded.
const SKIPPED_TOTP_STEP: i64 = -1;

/// Records the accepted step atomically; a concurrent replay of the same step fails.
async fn record_totp_step(conn: &Connection, user_id: &str, step: i64) -> Result<(), AppError> {
    if step == SKIPPED_TOTP_STEP {
        return Ok(());
    }
    let changed = conn
        .execute(
            "UPDATE user_credentials SET totp_last_step = ?2
             WHERE user_id = ?1 AND (totp_last_step IS NULL OR totp_last_step < ?2)",
            (user_id, step),
        )
        .await?;
    if changed == 1 {
        Ok(())
    } else {
        Err(invalid_totp())
    }
}

/// Audits a failed attempt (own transaction, committed even though the call fails).
async fn audit_failure(deps: &Deps<'_>, user_id: Option<&str>, step: &str) -> Result<(), AppError> {
    let tx = deps.db.write().await?;
    audit::append(
        &tx,
        deps.now,
        Event {
            kind: "identity.auth_failed",
            visibility: Visibility::Restricted,
            actor_ref: None,
            subject: user_id,
            payload: serde_json::json!({ "step": step }),
        },
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

/// A rejected factor: the audited step and the generic error returned.
type Rejected = (&'static str, AppError);

/// Face challenge consumed in the reservation transaction of [`attempt_deferred`].
struct FaceChallenge<'a> {
    subject: &'a str,
    challenge: &'a str,
}

/// Runs one rate-limited verification. The attempt is counted before `verify`
/// runs (atomic with the limit check, and with the face challenge consumption
/// when given), kept and audited when a factor is rejected, and released when
/// verification cannot run. On success the caller releases the returned attempt
/// inside its effect transaction (one write transaction fewer per step).
async fn attempt_deferred<T>(
    deps: &Deps<'_>,
    keys: &ClientKeys,
    user_id: Option<&str>,
    face: Option<FaceChallenge<'_>>,
    verify: impl Future<Output = Result<Result<T, Rejected>, AppError>>,
) -> Result<(T, ratelimit::Attempt), AppError> {
    let tx = deps.db.write().await?;
    ratelimit::check(&tx, deps.now, keys).await?;
    if let Some(face) = face {
        consume_face_challenge_in(&tx, deps.now, face.subject, face.challenge).await?;
    }
    let reserved = ratelimit::record_failure(&tx, deps.now, keys).await?;
    tx.commit().await?;
    match verify.await {
        Ok(Ok(value)) => Ok((value, reserved)),
        Ok(Err((step, error))) => {
            audit_failure(deps, user_id, step).await?;
            Err(error)
        }
        Err(error) => {
            release_now(deps, reserved).await?;
            Err(error)
        }
    }
}

async fn release_now(deps: &Deps<'_>, reserved: ratelimit::Attempt) -> Result<(), AppError> {
    let tx = deps.db.write().await?;
    ratelimit::release(&tx, reserved).await?;
    tx.commit().await?;
    Ok(())
}

/// Verifies password (optional), face and TOTP for a signed-in account. Failures are
/// counted per user (never per e-mail/IP, see `ratelimit`). Returns the TOTP step to
/// record and the attempt to release in the effect transaction.
async fn verify_session_factors(
    deps: &Deps<'_>,
    user_id: &str,
    password: Option<&str>,
    proof: &FaceProof<'_>,
    totp_code: &str,
) -> Result<(i64, ratelimit::Attempt), AppError> {
    let conn = deps.db.read().await?;
    if credentials(&conn, user_id).await?.totp_reenroll_required {
        return Err(AppError::Conflict(t("auth.error.reenroll_totp").into()));
    }
    let keys = ClientKeys::session(user_id);
    let subject = user_subject(user_id);
    // Without a password the challenge is consumed with the reservation; with one,
    // only after the password matched (a wrong password keeps the capture usable).
    let face = password.is_none().then(|| FaceChallenge {
        subject: &subject,
        challenge: proof.challenge,
    });
    attempt_deferred(deps, &keys, Some(user_id), face, async {
        if let Some(password) = password {
            let stored: Option<String> = query_opt(
                &conn,
                "SELECT password_hash FROM users WHERE id = ?1",
                [user_id],
            )
            .await?
            .map(|r| r.get(0))
            .transpose()?;
            if !crate::infrastructure::password::verify(stored, password).await {
                return Ok(Err(("password", invalid_credentials())));
            }
            if !verify_face(deps, &subject, Some(user_id), proof).await? {
                return Ok(Err(("face", face_not_confirmed())));
            }
        } else if !match_face(deps, Some(user_id), proof).await? {
            return Ok(Err(("face", face_not_confirmed())));
        }
        Ok(check_totp(deps, user_id, totp_code)
            .await?
            .ok_or_else(|| ("totp", invalid_totp())))
    })
    .await
}

async fn user_email(conn: &Connection, user_id: &str) -> Result<String, AppError> {
    Ok(
        query_opt(conn, "SELECT email FROM users WHERE id = ?1", [user_id])
            .await?
            .ok_or(AppError::NotFound)?
            .get(0)?,
    )
}

/// Recovery code: 128 random bits shown as eight groups of four hex digits.
fn new_recovery_code() -> String {
    let hex = random_token(16);
    hex.as_bytes()
        .chunks(4)
        .map(|c| std::str::from_utf8(c).expect("hex"))
        .collect::<Vec<_>>()
        .join("-")
}

fn recovery_code_hash(code: &str) -> Option<String> {
    let normalized: String = code
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .collect::<String>()
        .to_lowercase();
    (normalized.len() == 32 && normalized.chars().all(|c| c.is_ascii_hexdigit()))
        .then(|| sha256_hex(format!("recovery:{normalized}")))
}

pub const RECOVERY_CODES: usize = 10;

/// Replaces all active codes of the user with ten new ones (returned once).
async fn replace_recovery_codes(
    conn: &Connection,
    now: DateTime<Utc>,
    user_id: &str,
) -> Result<Vec<String>, AppError> {
    conn.execute(
        "UPDATE recovery_codes SET revoked_at = ?2 WHERE user_id = ?1 AND used_at IS NULL AND revoked_at IS NULL",
        (user_id, ts(now)),
    )
    .await?;
    let mut codes = Vec::with_capacity(RECOVERY_CODES);
    for _ in 0..RECOVERY_CODES {
        let code = new_recovery_code();
        let hash = recovery_code_hash(&code)
            .ok_or_else(|| AppError::internal("missing or invalid stored value"))?;
        conn.execute(
            "INSERT INTO recovery_codes (code_hash, user_id, created_at) VALUES (?1, ?2, ?3)",
            (hash, user_id, ts(now)),
        )
        .await?;
        codes.push(code);
    }
    Ok(codes)
}

#[cfg(test)]
pub mod testing {
    //! Fixtures for identity tests: mini catalog, deps and an enrolled account.
    use super::*;
    use crate::infrastructure::{catalog, db::testing::TestDb};

    pub const MINI_CATALOG: &str = "code,kind,name,parent_code,state_abbrev\nBR,national,Brasil,,\n42,state,Santa Catarina,BR,SC\n41,state,Paraná,BR,PR\n4202404,municipal,Blumenau,42,SC\n4205407,municipal,Florianópolis,42,SC\n4106902,municipal,Curitiba,41,PR\n";
    pub const PASSWORD: &str = "senha de teste longa";
    pub const FACE: &[u8] = b"rosto sintetico de teste";

    pub struct World {
        pub t: TestDb,
        pub face: FaceVerifier,
        pub secrets: SecretBox,
        pub now: DateTime<Utc>,
    }

    impl World {
        pub async fn new() -> Self {
            let t = crate::infrastructure::db::testing::migrated().await;
            catalog::import_str(&t.db, "mini", MINI_CATALOG)
                .await
                .unwrap();
            Self {
                t,
                face: FaceVerifier::Synthetic,
                secrets: SecretBox::from_key(&[9; 32]),
                now: "2026-10-24T12:00:00Z".parse().unwrap(),
            }
        }

        pub fn deps(&self) -> Deps<'_> {
            Deps {
                db: &self.t.db,
                now: self.now,
                face: &self.face,
                secrets: &self.secrets,
                policy: SessionPolicy::from_secs(900, 8 * 3600),
                skip_totp: false,
            }
        }

        pub fn at(&self, now: DateTime<Utc>) -> Deps<'_> {
            Deps { now, ..self.deps() }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovery_codes_have_128_bits_and_normalize() {
        let code = new_recovery_code();
        assert_eq!(code.len(), 39);
        assert_eq!(code.split('-').count(), 8);
        let h = recovery_code_hash(&code).unwrap();
        assert_eq!(recovery_code_hash(&code.to_uppercase()), Some(h.clone()));
        assert_eq!(recovery_code_hash(&code.replace('-', " ")), Some(h));
        assert_eq!(recovery_code_hash("abcd"), None);
        assert_ne!(new_recovery_code(), code);
    }
}
