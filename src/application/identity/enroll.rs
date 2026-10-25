//! Enrollment after registration: face, then TOTP confirmed with a code, then ten
//! recovery codes shown once. The account becomes active only after face + TOTP.
//! The same TOTP steps serve the forced re-enrollment after a recovery code login.

use super::{
    Deps, FaceProof, consume_face_challenge, credentials, face_aad, replace_recovery_codes,
    totp_aad, user_email, user_subject,
};
use crate::{
    application::{
        actor::Actor,
        audit::{self, Event, Visibility},
        session,
    },
    domain::identity::AccountStatus,
    error::AppError,
    i18n::t,
    infrastructure::{
        clock::ts,
        face::FaceSample,
        totp::{self, Provisioning},
    },
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    pub face_enrolled: bool,
    pub totp_confirmed: bool,
    pub totp_reenroll_required: bool,
}

#[tracing::instrument(level = "debug", skip_all, fields(user_id = %user_id), err(level = "debug"))]
pub async fn status(deps: &Deps<'_>, user_id: &str) -> Result<Status, AppError> {
    let conn = deps.db.read().await?;
    let c = credentials(&conn, user_id).await?;
    Ok(Status {
        face_enrolled: c.face_template.is_some(),
        totp_confirmed: c.totp_secret.is_some(),
        totp_reenroll_required: c.totp_reenroll_required,
    })
}

fn not_allowed() -> AppError {
    AppError::Conflict(t("auth.enroll.not_allowed").into())
}

/// Stores the encrypted face template of a pending account (once).
#[tracing::instrument(level = "debug", skip_all, fields(actor = %actor.user_id), err(level = "debug"))]
pub async fn enroll_face(
    deps: &Deps<'_>,
    actor: &Actor,
    proof: FaceProof<'_>,
) -> Result<(), AppError> {
    if actor.status != AccountStatus::Pending || status(deps, &actor.user_id).await?.face_enrolled {
        return Err(not_allowed());
    }
    consume_face_challenge(
        deps.db,
        deps.now,
        &user_subject(&actor.user_id),
        proof.challenge,
    )
    .await?;
    let template = deps
        .face
        .enroll(FaceSample {
            image: proof.image,
            challenge: proof.challenge,
        })
        .await?;
    let sealed = deps.secrets.seal(&template.0, &face_aad(&actor.user_id))?;
    let tx = deps.db.write().await?;
    let changed = tx
        .execute(
            "INSERT INTO user_credentials (user_id, face_template, face_enrolled_at, updated_at) VALUES (?1, ?2, ?3, ?3)
             ON CONFLICT(user_id) DO UPDATE SET face_template = excluded.face_template,
               face_enrolled_at = excluded.face_enrolled_at, updated_at = excluded.updated_at
             WHERE user_credentials.face_template IS NULL",
            (actor.user_id.as_str(), sealed, ts(deps.now)),
        )
        .await?;
    if changed != 1 {
        return Err(not_allowed());
    }
    tx.commit().await?;
    Ok(())
}

/// New pending TOTP secret (replaces a previous unconfirmed one).
#[tracing::instrument(level = "debug", skip_all, fields(actor = %actor.user_id), err(level = "debug"))]
pub async fn begin_totp(deps: &Deps<'_>, actor: &Actor) -> Result<Provisioning, AppError> {
    let s = status(deps, &actor.user_id).await?;
    let first = actor.status == AccountStatus::Pending && s.face_enrolled && !s.totp_confirmed;
    if !(first || s.totp_reenroll_required) {
        return Err(not_allowed());
    }
    let secret = totp::new_secret();
    let sealed = deps.secrets.seal(&secret, &totp_aad(&actor.user_id))?;
    let tx = deps.db.write().await?;
    tx.execute(
        "UPDATE user_credentials SET totp_pending_secret = ?2, updated_at = ?3 WHERE user_id = ?1",
        (actor.user_id.as_str(), sealed, ts(deps.now)),
    )
    .await?;
    let email = user_email(&tx, &actor.user_id).await?;
    tx.commit().await?;
    totp::provisioning(&secret, &email)
}

pub struct Confirmed {
    /// Shown once; only for the first enrollment.
    pub recovery_codes: Option<Vec<String>>,
    /// New session token when activation changed privileges.
    pub session_token: Option<String>,
}

/// Confirms the pending secret with a valid code. First enrollment activates the
/// account, issues recovery codes and rotates the session; re-enrollment clears the flag.
#[tracing::instrument(level = "debug", skip_all, fields(actor = %actor.user_id), err(level = "debug"))]
pub async fn confirm_totp(
    deps: &Deps<'_>,
    actor: &Actor,
    code: &str,
) -> Result<Confirmed, AppError> {
    let conn = deps.db.read().await?;
    let creds = credentials(&conn, &actor.user_id).await?;
    let first = actor.status == AccountStatus::Pending
        && creds.face_template.is_some()
        && creds.totp_secret.is_none();
    if !(first || creds.totp_reenroll_required) {
        return Err(not_allowed());
    }
    let sealed = creds.totp_pending_secret.ok_or_else(not_allowed)?;
    let secret = deps.secrets.open(&sealed, &totp_aad(&actor.user_id))?;
    let step = if deps.skip_totp {
        None
    } else {
        Some(
            totp::verify(&secret, code, deps.now.timestamp(), None)
                .ok_or_else(super::invalid_totp)?,
        )
    };

    let tx = deps.db.write().await?;
    let changed = tx
        .execute(
            "UPDATE user_credentials SET totp_secret = totp_pending_secret, totp_pending_secret = NULL,
               totp_confirmed_at = ?2, totp_last_step = ?3, totp_reenroll_required = 0, updated_at = ?2
             WHERE user_id = ?1 AND totp_pending_secret = ?4",
            (actor.user_id.as_str(), ts(deps.now), step, sealed),
        )
        .await?;
    if changed != 1 {
        return Err(not_allowed());
    }
    let (kind, recovery_codes, session_token) = if first {
        tx.execute(
            "UPDATE users SET status = 'active', updated_at = ?2 WHERE id = ?1 AND status = 'pending'",
            (actor.user_id.as_str(), ts(deps.now)),
        )
        .await?;
        let codes = replace_recovery_codes(&tx, deps.now, &actor.user_id).await?;
        session::revoke(&tx, deps.now, &actor.session_hash, "privilege_change").await?;
        let token = session::create(&tx, deps.now, deps.policy, &actor.user_id).await?;
        ("identity.enrollment_completed", Some(codes), Some(token))
    } else {
        ("identity.totp_reenrolled", None, None)
    };
    audit::append(
        &tx,
        deps.now,
        Event {
            kind,
            visibility: Visibility::Restricted,
            actor_ref: Some(&actor.user_id),
            subject: Some(&actor.user_id),
            payload: serde_json::json!({}),
        },
    )
    .await?;
    tx.commit().await?;
    Ok(Confirmed {
        recovery_codes,
        session_token,
    })
}

#[cfg(test)]
pub mod tests {
    use super::{
        super::{issue_face_challenge, register, testing::*},
        *,
    };
    use crate::{
        application::{actor, session::token_hash},
        domain::identity::Permission,
        infrastructure::{
            address::AddressProvider, address::testing::mock_brasilapi, db::query_opt,
        },
    };

    pub struct Enrolled {
        pub user_id: String,
        pub email: String,
        pub session_token: String,
        pub totp_secret: Vec<u8>,
        pub recovery_codes: Vec<String>,
    }

    /// A different valid-looking code.
    pub fn wrong_code(code: &str) -> String {
        code.chars()
            .map(|c| char::from(b'0' + (c as u8 - b'0' + 1) % 10))
            .collect()
    }

    pub async fn actor_for(w: &World, token: &str) -> Actor {
        let conn = w.t.db.read().await.unwrap();
        let user: String = query_opt(
            &conn,
            "SELECT user_id FROM sessions WHERE id_hash = ?1",
            [token_hash(token)],
        )
        .await
        .unwrap()
        .unwrap()
        .get(0)
        .unwrap();
        actor::load(&conn, &user, &token_hash(token))
            .await
            .unwrap()
            .unwrap()
    }

    pub async fn face_proof_challenge(w: &World, user_id: &str) -> String {
        issue_face_challenge(&w.t.db, w.now, &user_subject(user_id))
            .await
            .unwrap()
    }

    /// Registers and fully enrolls an account (synthetic face `FACE`).
    pub async fn enrolled(w: &World, email: &str) -> Enrolled {
        let p = AddressProvider::brasilapi(
            &mock_brasilapi().await,
            std::time::Duration::from_millis(500),
        );
        let r = register::register(
            &w.deps(),
            Some(&p),
            "BR",
            register::Registration {
                name: "Ana Sintética",
                email,
                password: PASSWORD,
                cep: "89010025",
            },
        )
        .await
        .unwrap();
        let a = actor_for(w, &r.session_token).await;
        let c = face_proof_challenge(w, &a.user_id).await;
        enroll_face(
            &w.deps(),
            &a,
            FaceProof {
                challenge: &c,
                image: FACE,
            },
        )
        .await
        .unwrap();
        let prov = begin_totp(&w.deps(), &a).await.unwrap();
        let secret = totp_rs::Secret::try_from_base32(&prov.secret_base32)
            .unwrap()
            .as_bytes()
            .to_vec();
        // Confirm in the previous step so later tests can use the current one.
        let earlier = w.at(w.now - chrono::Duration::seconds(30));
        let done = confirm_totp(
            &earlier,
            &a,
            &totp::code_at(&secret, earlier.now.timestamp()),
        )
        .await
        .unwrap();
        Enrolled {
            user_id: a.user_id,
            email: email.to_owned(),
            session_token: done.session_token.unwrap(),
            totp_secret: secret,
            recovery_codes: done.recovery_codes.unwrap(),
        }
    }

    #[tokio::test]
    async fn dev_skip_totp_confirms_enrollment_without_code() {
        let w = World::new().await;
        let p = AddressProvider::brasilapi(
            &mock_brasilapi().await,
            std::time::Duration::from_millis(500),
        );
        let r = register::register(
            &w.deps(),
            Some(&p),
            "BR",
            register::Registration {
                name: "Ana Sintética",
                email: "ana@example.org",
                password: PASSWORD,
                cep: "89010025",
            },
        )
        .await
        .unwrap();
        let a = actor_for(&w, &r.session_token).await;
        let c = face_proof_challenge(&w, &a.user_id).await;
        let proof = FaceProof {
            challenge: &c,
            image: FACE,
        };
        enroll_face(&w.deps(), &a, proof).await.unwrap();
        begin_totp(&w.deps(), &a).await.unwrap();
        let skip = Deps {
            skip_totp: true,
            ..w.deps()
        };
        let done = confirm_totp(&skip, &a, "").await.unwrap();
        assert_eq!(done.recovery_codes.unwrap().len(), 10);
        assert!(status(&w.deps(), &a.user_id).await.unwrap().totp_confirmed);
    }

    #[tokio::test]
    async fn activation_requires_face_and_totp() {
        let w = World::new().await;
        let p = AddressProvider::brasilapi(
            &mock_brasilapi().await,
            std::time::Duration::from_millis(500),
        );
        let r = register::register(
            &w.deps(),
            Some(&p),
            "BR",
            register::Registration {
                name: "Bia",
                email: "bia@example.org",
                password: PASSWORD,
                cep: "89010025",
            },
        )
        .await
        .unwrap();
        let a = actor_for(&w, &r.session_token).await;
        assert_eq!(
            begin_totp(&w.deps(), &a).await.err(),
            Some(not_allowed()),
            "face first"
        );
        // Challenge is single use and bound to the subject.
        let c = face_proof_challenge(&w, &a.user_id).await;
        let other = issue_face_challenge(&w.t.db, w.now, "user:someone-else")
            .await
            .unwrap();
        assert!(
            enroll_face(
                &w.deps(),
                &a,
                FaceProof {
                    challenge: &other,
                    image: FACE
                }
            )
            .await
            .is_err()
        );
        enroll_face(
            &w.deps(),
            &a,
            FaceProof {
                challenge: &c,
                image: FACE,
            },
        )
        .await
        .unwrap();
        let c2 = face_proof_challenge(&w, &a.user_id).await;
        assert!(
            enroll_face(
                &w.deps(),
                &a,
                FaceProof {
                    challenge: &c2,
                    image: b"outro"
                }
            )
            .await
            .is_err(),
            "once"
        );

        let prov = begin_totp(&w.deps(), &a).await.unwrap();
        let secret = totp_rs::Secret::try_from_base32(&prov.secret_base32)
            .unwrap()
            .as_bytes()
            .to_vec();
        let wrong = wrong_code(&totp::code_at(&secret, w.now.timestamp()));
        assert!(confirm_totp(&w.deps(), &a, &wrong).await.is_err());
        assert!(!status(&w.deps(), &a.user_id).await.unwrap().totp_confirmed);
        let done = confirm_totp(&w.deps(), &a, &totp::code_at(&secret, w.now.timestamp()))
            .await
            .unwrap();
        let codes = done.recovery_codes.unwrap();
        assert_eq!(codes.len(), 10);

        // Old session revoked; new one carries citizen permissions.
        assert!(
            session::resolve(&w.t.db, w.now, w.deps().policy, &r.session_token, false)
                .await
                .is_err()
        );
        let active = actor_for(&w, done.session_token.as_ref().unwrap()).await;
        assert_eq!(active.status, AccountStatus::Active);
        assert!(active.can(Permission::ProposalVote));

        // Secrets encrypted at rest; codes stored only as hashes.
        let conn = w.t.db.read().await.unwrap();
        let row = query_opt(
            &conn,
            "SELECT face_template, totp_secret FROM user_credentials WHERE user_id = ?1",
            [a.user_id.as_str()],
        )
        .await
        .unwrap()
        .unwrap();
        let face: Vec<u8> = row.get(0).unwrap();
        let stored_secret: Vec<u8> = row.get(1).unwrap();
        let template = crate::infrastructure::crypto::sha256_hex(FACE);
        assert!(!face.windows(64).any(|w| w == template.as_bytes()));
        assert!(
            !stored_secret
                .windows(secret.len())
                .any(|w| w == secret.as_slice())
        );
        let plain = query_opt(
            &conn,
            "SELECT 1 FROM recovery_codes WHERE code_hash = ?1",
            [codes[0].as_str()],
        )
        .await
        .unwrap();
        assert!(plain.is_none());
    }
}
