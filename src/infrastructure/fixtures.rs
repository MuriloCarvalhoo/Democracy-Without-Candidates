//! Synthetic development/test fixtures. Refused in production. Never creates fixed
//! production credentials; feature modules add their own synthetic data here.

use super::{
    clock::ts,
    config::Config,
    crypto::new_id,
    db::{Db, query_opt},
    flags,
};
use crate::{
    application::{
        actor, phases,
        proposals::{self, Ctx},
        reauth,
        session::{self, SessionPolicy},
        setup, signatures,
    },
    domain::{
        proposals::{Level, ProposalKind, REAUTH_PUBLISH, REAUTH_SIGN, view::DraftInput},
        setup::SUGGESTED,
    },
    error::AppError,
};

/// Flags enabled for local demonstration with synthetic data.
pub const DEV_FLAGS: &[&str] = &[
    flags::ADDRESS_BRASILAPI,
    flags::PROPOSALS,
    flags::SIGNATURES,
    flags::DEBATE,
    flags::VOTING,
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
    done.extend(seed_legislative(db, config, chrono::Utc::now()).await?);
    Ok(done)
}

/// Synthetic citizens (unusable passwords), a synthetic national setup with the
/// suggested package and sample proposals in several phases. Idempotent.
async fn seed_legislative(
    db: &Db,
    config: &Config,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<Vec<String>, AppError> {
    let conn = db.read().await?;
    if query_opt(
        &conn,
        "SELECT 1 FROM setup_versions WHERE synthetic = 1",
        (),
    )
    .await?
    .is_some()
    {
        return Ok(vec!["legislative fixtures already present".into()]);
    }
    let territory = |code: &'static str| {
        let conn = &conn;
        async move {
            let row = query_opt(
                conn,
                "SELECT m.id, s.id, s.parent_id FROM jurisdictions m JOIN jurisdictions s ON s.id = m.parent_id WHERE m.code = ?1",
                [code],
            )
            .await?
            .ok_or(AppError::NotFound)?;
            Ok::<_, AppError>((
                row.get::<String>(0)?,
                row.get::<String>(1)?,
                row.get::<String>(2)?,
            ))
        }
    };
    let blumenau = territory("4202404").await?;
    let floripa = territory("4205407").await?;

    let policy = SessionPolicy::from_secs(900, 28800);
    let tx = db.write().await?;
    setup::adopt_setup(&tx, now, &blumenau.2, &SUGGESTED, None, true).await?;
    let mut citizens = Vec::new();
    for n in 1..=16 {
        let (m, s, c) = if n <= 12 { &blumenau } else { &floripa };
        let id = new_id();
        tx.execute(
            "INSERT INTO users (id, email, name, password_hash, status, municipality_id, state_id, country_id, synthetic, created_at, updated_at)
             VALUES (?1, ?2, ?3, '!synthetic-no-login', 'active', ?4, ?5, ?6, 1, ?7, ?7)",
            (id.as_str(), format!("sintetico-{n:02}@dev.invalid"), format!("Pessoa Sintética {n:02}"), m.as_str(), s.as_str(), c.as_str(), ts(now)),
        )
        .await?;
        tx.execute(
            "INSERT INTO user_roles (user_id, role, granted_at, granted_by) VALUES (?1, 'CITIZEN', ?2, 'seed-dev')",
            (id.as_str(), ts(now)),
        )
        .await?;
        let token = session::create(&tx, now, policy, &id).await?;
        citizens.push((id, session::token_hash(&token)));
    }
    tx.commit().await?;

    let flags = flags::Flags::load(db).await?;
    let tz = config.country.timezone.parse().unwrap_or(chrono_tz::UTC);
    let actor_of = |i: usize| {
        let (id, hash) = citizens[i].clone();
        async move {
            actor::load(&db.read().await?, &id, &hash)
                .await?
                .ok_or(AppError::Internal)
        }
    };
    let grant = |hash: String, action: &'static str, id: String, at| async move {
        let tx = db.write().await?;
        reauth::grant(&tx, at, &hash, action, &id).await?;
        tx.commit().await?;
        Ok::<_, AppError>(())
    };
    let samples: [(&str, ProposalKind, Level, usize, i64); 6] = [
        (
            "Ciclovia na Rua XV de Novembro",
            ProposalKind::NormalLaw,
            Level::Municipal,
            0,
            0,
        ),
        (
            "Horário estendido das bibliotecas estaduais",
            ProposalKind::NormalLaw,
            Level::State,
            1,
            0,
        ),
        (
            "Reforma da praça central",
            ProposalKind::NormalLaw,
            Level::Municipal,
            2,
            1,
        ),
        (
            "Plano emergencial contra enchentes",
            ProposalKind::UrgentLaw,
            Level::Municipal,
            3,
            1,
        ),
        (
            "Transparência dos contratos públicos",
            ProposalKind::NormalLaw,
            Level::National,
            4,
            -100,
        ),
        (
            "Feira de trocas no bairro",
            ProposalKind::NormalLaw,
            Level::Municipal,
            5,
            0,
        ),
    ];
    let mut done = vec!["synthetic national setup adopted (suggested package)".to_owned()];
    for (title, kind, level, author, shift) in samples {
        // Positive shift: one extra signature; negative: published in the past.
        let at = now + chrono::Duration::days(shift.min(0));
        let ctx = Ctx {
            db,
            flags: &flags,
            now: at,
            tz,
        };
        let a = actor_of(author).await?;
        let input = DraftInput {
            kind,
            level,
            title: format!("[Sintético] {title}"),
            summary: format!(
                "Proposta sintética de demonstração: {}.",
                title.to_lowercase()
            ),
            body: format!(
                "Texto sintético, sem validade eleitoral.\n\nArt. 1º {title}.\nArt. 2º Esta lei entra em vigor na data de sua publicação."
            ),
            proposed_setup: None,
        };
        let id = proposals::save_draft(&ctx, &a, None, &input).await?.id;
        grant(a.session_hash.clone(), REAUTH_PUBLISH, id.clone(), at).await?;
        proposals::publish(&ctx, &a, &id, None).await?;
        if shift > 0 {
            grant(a.session_hash.clone(), REAUTH_SIGN, id.clone(), at).await?;
            signatures::sign(&ctx, &a, &id, None).await?;
        }
        if title.starts_with("Feira") {
            proposals::withdraw(&ctx, &a, &id, "Retirada sintética de demonstração").await?;
        }
        done.push(format!("proposal {id}: {title}"));
    }
    phases::advance_due(db, now).await?;
    let tx = db.write().await?;
    for (_, hash) in &citizens {
        session::revoke(&tx, now, hash, "seed-dev").await?;
    }
    tx.commit().await?;
    Ok(done)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::{catalog, db::testing::migrated};

    fn config(env: &str) -> Config {
        let vars = [
            ("DWC_ENV", env),
            ("DWC_DB_PATH", "unused.db"),
            ("DWC_COUNTRY", "BR"),
            ("DWC_IDENTITY_PROVIDER", "face-service"),
            ("DWC_FACE_SERVICE_URL", "http://127.0.0.1:8001"),
            ("DWC_PUBLIC_ORIGIN", "https://br.example.invalid"),
            ("DWC_SECRET_KEY_FILE", "/nonexistent"),
        ];
        Config::from_lookup("127.0.0.1:3000".parse().unwrap(), |k| {
            vars.iter()
                .find(|(n, _)| *n == k)
                .map(|(_, v)| v.to_string())
        })
        .unwrap()
    }

    #[tokio::test]
    async fn seeds_synthetic_proposals_once_and_refuses_production() {
        let t = migrated().await;
        catalog::import_str(&t.db, "mini", "code,kind,name,parent_code,state_abbrev\nBR,national,Brasil,,\n42,state,Santa Catarina,BR,SC\n4202404,municipal,Blumenau,42,SC\n4205407,municipal,Florianópolis,42,SC\n").await.unwrap();
        assert_eq!(
            seed_dev(&t.db, &config("production")).await,
            Err(AppError::Forbidden)
        );
        seed_dev(&t.db, &config("development")).await.unwrap();
        let again = seed_dev(&t.db, &config("development")).await.unwrap();
        assert!(again.iter().any(|l| l.contains("already present")));
        let conn = t.db.read().await.unwrap();
        let mut statuses = Vec::new();
        for row in crate::infrastructure::db::query_all(
            &conn,
            "SELECT status FROM proposals ORDER BY status",
            (),
        )
        .await
        .unwrap()
        {
            statuses.push(row.get::<String>(0).unwrap());
        }
        assert_eq!(
            statuses,
            [
                "archived",
                "collecting_signatures",
                "collecting_signatures",
                "expired",
                "in_debate",
                "in_voting"
            ]
        );
        let open: i64 = query_opt(
            &conn,
            "SELECT COUNT(*) FROM sessions WHERE revoked_at IS NULL",
            (),
        )
        .await
        .unwrap()
        .unwrap()
        .get(0)
        .unwrap();
        assert_eq!(open, 0, "seed sessions are revoked");
    }
}
