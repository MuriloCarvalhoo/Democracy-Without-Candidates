//! Synthetic development/test fixtures. Refused in production. Never creates fixed
//! production credentials; feature modules add their own synthetic data here.

use super::{
    clock::ts,
    config::Config,
    crypto::new_id,
    db::{Db, query_opt},
    face::{FaceSample, FaceVerifier},
    flags, password,
    secrets::SecretBox,
};
use crate::error::AppError;

/// Flags enabled for local demonstration with synthetic data.
pub const DEV_FLAGS: &[&str] = &[
    flags::ADDRESS_BRASILAPI,
    flags::PROPOSALS,
    flags::SIGNATURES,
    flags::DEBATE,
    flags::VOTING,
];

/// Development-only credentials shared by every synthetic account (documented,
/// printed by `seed-dev`; the command refuses to run in production).
pub const DEV_PASSWORD: &str = "senha-sintetica-dev";
pub const DEV_FACE_PHRASE: &str = "rosto-sintetico-dev";
/// Base32 TOTP secret (20 bytes) of every synthetic account.
pub const DEV_TOTP_SECRET: &str = "JBSWY3DPEHPK3PXPJBSWY3DPEHPK3PXP";

/// (e-mail, name, municipality IBGE code, role).
pub const DEV_ACCOUNTS: &[(&str, &str, &str, &str)] = &[
    ("ana@dev.invalid", "Ana Sintética", "4202404", "CITIZEN"),
    ("bruno@dev.invalid", "Bruno Sintético", "4202404", "CITIZEN"),
    ("carla@dev.invalid", "Carla Sintética", "3550308", "CITIZEN"),
    ("davi@dev.invalid", "Davi Sintético", "3304557", "CITIZEN"),
    (
        "admin@dev.invalid",
        "Admin País Sintético",
        "5300108",
        "COUNTRY_ADMIN",
    ),
    (
        "auditor@dev.invalid",
        "Auditoria Sintética",
        "5300108",
        "AUDITOR",
    ),
];

pub async fn seed_dev(db: &Db, config: &Config) -> Result<Vec<String>, AppError> {
    if config.is_production() {
        return Err(AppError::Forbidden);
    }
    let mut done = Vec::new();
    for key in DEV_FLAGS {
        flags::store(db, key, true, Some("seed-dev")).await?;
        done.push(format!("flag {key}=on"));
    }
    let secrets = SecretBox::load(config).map_err(|e| {
        tracing::error!(%e, "secret key");
        AppError::Internal
    })?;
    done.extend(seed_accounts(db, &secrets).await?);
    Ok(done)
}

/// Active synthetic accounts with face (phrase) + TOTP enrolled. Idempotent.
pub async fn seed_accounts(db: &Db, secrets: &SecretBox) -> Result<Vec<String>, AppError> {
    let totp_secret = totp_rs::Secret::try_from_base32(DEV_TOTP_SECRET)
        .map_err(|_| AppError::Internal)?
        .as_bytes()
        .to_vec();
    let template = FaceVerifier::Synthetic
        .enroll(FaceSample {
            image: DEV_FACE_PHRASE.as_bytes(),
            challenge: "seed-dev",
        })
        .await?;
    let password_hash = password::hash(DEV_PASSWORD).await?;
    let now = ts(chrono::Utc::now());
    let mut out = vec![format!(
        "synthetic accounts (development only): password={DEV_PASSWORD} face phrase={DEV_FACE_PHRASE} totp secret={DEV_TOTP_SECRET}"
    )];
    for (email, name, municipality, role) in DEV_ACCOUNTS {
        let tx = db.write().await?;
        if query_opt(&tx, "SELECT 1 FROM users WHERE email = ?1", [*email])
            .await?
            .is_some()
        {
            tx.rollback().await?;
            out.push(format!("account {email} exists"));
            continue;
        }
        let row = query_opt(
            &tx,
            "SELECT m.id, s.id, s.parent_id, m.name, s.state_abbrev, s.code FROM jurisdictions m
             JOIN jurisdictions s ON s.id = m.parent_id WHERE m.code = ?1",
            [*municipality],
        )
        .await?
        .ok_or(AppError::NotFound)?;
        let (m, s, c, city, uf, state_code): (String, String, String, String, String, String) = (
            row.get(0)?,
            row.get(1)?,
            row.get(2)?,
            row.get(3)?,
            row.get(4)?,
            row.get(5)?,
        );
        let id = new_id();
        tx.execute(
            "INSERT INTO users (id, email, name, password_hash, status, municipality_id, state_id, country_id, synthetic, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, 'active', ?5, ?6, ?7, 1, ?8, ?8)",
            (id.as_str(), *email, *name, password_hash.as_str(), m, s, c, now.as_str()),
        )
        .await?;
        tx.execute(
            "INSERT INTO user_roles (user_id, role, granted_at, granted_by) VALUES (?1, ?2, ?3, 'seed-dev')",
            (id.as_str(), *role, now.as_str()),
        )
        .await?;
        tx.execute(
            "INSERT INTO addresses (user_id, cep, state, city, neighborhood, street, service, ibge, created_at)
             VALUES (?1, '00000000', ?2, ?3, '', '', 'seed-dev', ?4, ?5)",
            (
                id.as_str(),
                uf,
                city.as_str(),
                serde_json::json!({ "city": municipality, "state": state_code }).to_string(),
                now.as_str(),
            ),
        )
        .await?;
        tx.execute(
            "INSERT INTO user_credentials (user_id, face_template, face_enrolled_at, totp_secret, totp_confirmed_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?3, ?3)",
            (
                id.as_str(),
                secrets.seal(&template.0, &format!("face:{id}"))?,
                now.as_str(),
                secrets.seal(&totp_secret, &format!("totp:{id}"))?,
            ),
        )
        .await?;
        tx.execute(
            "INSERT INTO user_preferences (user_id, updated_at) VALUES (?1, ?2)",
            (id.as_str(), now.as_str()),
        )
        .await?;
        tx.commit().await?;
        out.push(format!("account {email} ({role}, {city})"));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        application::identity::{
            FaceProof,
            auth::{self, LoginOutcome, Purpose, auth_subject},
            issue_face_challenge,
            testing::World,
        },
        infrastructure::{catalog, totp},
    };

    #[tokio::test]
    async fn seeded_accounts_sign_in_with_documented_factors() {
        let w = World::new().await;
        catalog::import(&w.t.db, "catalog/br/jurisdictions.csv")
            .await
            .unwrap();
        let first = seed_accounts(&w.t.db, &w.secrets).await.unwrap();
        assert_eq!(first.len(), DEV_ACCOUNTS.len() + 1);
        let again = seed_accounts(&w.t.db, &w.secrets).await.unwrap();
        assert!(
            again[1..].iter().all(|l| l.ends_with("exists")),
            "idempotent"
        );

        let d = w.deps();
        let (token, next) = auth::login_password(&d, None, "admin@dev.invalid", DEV_PASSWORD)
            .await
            .unwrap();
        assert_eq!(next, LoginOutcome::NeedFace);
        let token = token.unwrap();
        let c = issue_face_challenge(&w.t.db, w.now, &auth_subject(&token))
            .await
            .unwrap();
        auth::auth_face(
            &d,
            None,
            &token,
            Purpose::Login,
            FaceProof {
                challenge: &c,
                image: DEV_FACE_PHRASE.as_bytes(),
            },
        )
        .await
        .unwrap();
        let secret = totp_rs::Secret::try_from_base32(DEV_TOTP_SECRET)
            .unwrap()
            .as_bytes()
            .to_vec();
        auth::login_totp(&d, None, &token, &totp::code_at(&secret, w.now.timestamp()))
            .await
            .unwrap();
    }
}
