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
use crate::{
    application::{
        actor, debate, phases,
        proposals::{self, Ctx},
        reauth,
        session::{self, SessionPolicy},
        setup, signatures, voting,
    },
    domain::{
        proposals::{Level, ProposalKind, REAUTH_PUBLISH, REAUTH_SIGN, view::DraftInput},
        setup::SUGGESTED,
        voting::{Choice, REAUTH_VOTE},
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
    done.extend(seed_legislative(db, config, chrono::Utc::now()).await?);
    Ok(done)
}

/// Active synthetic accounts with face (phrase) + TOTP enrolled. Idempotent.
pub async fn seed_accounts(db: &Db, secrets: &SecretBox) -> Result<Vec<String>, AppError> {
    let totp_secret = totp_rs::Secret::try_from_base32(DEV_TOTP_SECRET)
        .map_err(AppError::internal)?
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
                .ok_or_else(|| AppError::internal("missing or invalid stored value"))
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
    let mut ids = Vec::new();
    for (title, kind, level, author, shift) in samples {
        // Positive shift: one extra signature; negative: published in the past.
        let at = now + chrono::Duration::days(shift.min(0));
        let ctx = Ctx {
            db,
            flags: &flags,
            now: at,
            clock: None,
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
        ids.push((title, id));
    }
    // Votes: a national urgent law open to every seeded login account, and a
    // municipal one that already closed with synthetic ballots and a result.
    let votes: [(&str, Level, usize, i64); 2] = [
        ("Dados abertos de despesas nacionais", Level::National, 6, 0),
        ("Iluminação pública em LED", Level::Municipal, 7, -10),
    ];
    for (title, level, author, shift) in votes {
        let at = now + chrono::Duration::days(shift);
        let ctx = Ctx {
            db,
            flags: &flags,
            now: at,
            clock: None,
            tz,
        };
        let a = actor_of(author).await?;
        let input = DraftInput {
            kind: ProposalKind::UrgentLaw,
            level,
            title: format!("[Sintético] {title}"),
            summary: format!(
                "Votação sintética de demonstração: {}.",
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
        grant(a.session_hash.clone(), REAUTH_SIGN, id.clone(), at).await?;
        signatures::sign(&ctx, &a, &id, None).await?;
        if shift < 0 {
            let cast_at = at + chrono::Duration::days(1);
            let ctx = Ctx {
                db,
                flags: &flags,
                now: cast_at,
                clock: None,
                tz,
            };
            let choices = [
                Choice::Yes,
                Choice::Yes,
                Choice::Yes,
                Choice::No,
                Choice::Abstain,
            ];
            for (i, choice) in choices.into_iter().enumerate() {
                let v = actor_of(i).await?;
                grant(v.session_hash.clone(), REAUTH_VOTE, id.clone(), cast_at).await?;
                voting::cast(&ctx, &v, &id, choice, true, None).await?;
            }
        }
        done.push(format!("vote {id}: {title}"));
    }
    phases::advance_due(db, now).await?;
    let id_of = |prefix: &str| {
        ids.iter()
            .find(|(t, _)| t.starts_with(prefix))
            .map(|(_, id)| id.clone())
            .ok_or_else(|| AppError::internal("missing or invalid stored value"))
    };
    done.push(
        seed_debate(
            db,
            &flags,
            tz,
            now,
            &actor_of,
            &id_of("Reforma")?,
            &id_of("Ciclovia")?,
        )
        .await?,
    );
    let tx = db.write().await?;
    for (_, hash) in &citizens {
        session::revoke(&tx, now, hash, "seed-dev").await?;
    }
    tx.commit().await?;
    Ok(done)
}

/// Synthetic comments, a reply chain and an amendment on seeded proposals.
async fn seed_debate<F, Fut>(
    db: &Db,
    flags: &flags::Flags,
    tz: chrono_tz::Tz,
    now: chrono::DateTime<chrono::Utc>,
    actor_of: &F,
    in_debate: &str,
    collecting: &str,
) -> Result<String, AppError>
where
    F: Fn(usize) -> Fut,
    Fut: std::future::Future<Output = Result<crate::application::actor::Actor, AppError>>,
{
    let ctx = Ctx {
        db,
        flags,
        now,
        clock: None,
        tz,
    };
    let first = debate::post_comment(
        &ctx,
        &actor_of(0).await?,
        in_debate,
        None,
        "[Sintético] Sugiro incluir bancos com sombra e bebedouros na reforma.",
    )
    .await?;
    let reply = debate::post_comment(
        &ctx,
        &actor_of(1).await?,
        in_debate,
        Some(&first),
        "[Sintético] Concordo, e com rampas de acessibilidade.",
    )
    .await?;
    debate::post_comment(
        &ctx,
        &actor_of(2).await?,
        in_debate,
        Some(&reply),
        "[Sintético] Obrigado! Isso pode entrar numa nova versão da proposta.",
    )
    .await?;
    debate::post_comment(
        &ctx,
        &actor_of(3).await?,
        in_debate,
        None,
        "[Sintético] Qual é o custo estimado da obra?",
    )
    .await?;
    debate::propose_amendment(
        &ctx,
        &actor_of(4).await?,
        in_debate,
        "[Sintético] Incluir acessibilidade",
        "Art. 1º-A A reforma incluirá rampas e piso tátil em todos os acessos.",
    )
    .await?;
    debate::post_comment(
        &ctx,
        &actor_of(5).await?,
        collecting,
        None,
        "[Sintético] Uma ciclovia aqui ligaria o centro à universidade.",
    )
    .await?;
    Ok("debate fixtures: 5 comments, 1 amendment".into())
}

/// Load-test dataset (`dwc seed-load <n> <out.json>`), only with `DWC_ENV=test`
/// on a database without setup: a synthetic national setup (signature target of
/// one account), `n` synthetic voters spread over three municipalities, each with
/// a session and a `proposal:vote` reauth grant (valid 5 minutes) for one
/// national urgent law in voting, plus five laws in collection for list reads.
/// Stores the `proposals`, `signatures` and `voting` flags (read at app start).
/// Returns `{proposal_id, cookie_name, tokens}`; tokens are disposable test
/// credentials written only to the requested file.
pub async fn seed_load(db: &Db, config: &Config, n: usize) -> Result<serde_json::Value, AppError> {
    if config.env != super::config::Environment::Test {
        return Err(AppError::Forbidden);
    }
    // Signature target ceil(1 bp × (n + 1)) stays 1, so the author's signature opens voting.
    if !(1..=9_998).contains(&n) {
        return Err(AppError::validation("n", "1..=9998"));
    }
    let now = chrono::Utc::now();
    let conn = db.read().await?;
    if query_opt(&conn, "SELECT 1 FROM setup_versions", ())
        .await?
        .is_some()
    {
        return Err(AppError::Conflict("database already has a setup".into()));
    }
    let mut territories = Vec::new();
    for code in ["4202404", "3550308", "3304557"] {
        let row = query_opt(
            &conn,
            "SELECT m.id, s.id, s.parent_id FROM jurisdictions m JOIN jurisdictions s ON s.id = m.parent_id WHERE m.code = ?1",
            [code],
        )
        .await?
        .ok_or(AppError::NotFound)?;
        territories.push((
            row.get::<String>(0)?,
            row.get::<String>(1)?,
            row.get::<String>(2)?,
        ));
    }
    let policy = SessionPolicy::from_secs(config.session_idle_secs, config.session_absolute_secs);
    let params = crate::domain::setup::SetupParams {
        signature_bp_national: 1,
        ..SUGGESTED
    };
    let tx = db.write().await?;
    setup::adopt_setup(&tx, now, &territories[0].2, &params, None, true).await?;
    let mut accounts = Vec::with_capacity(n + 1);
    for i in 0..=n {
        let (m, s, c) = &territories[i % territories.len()];
        let id = new_id();
        tx.execute(
            "INSERT INTO users (id, email, name, password_hash, status, municipality_id, state_id, country_id, synthetic, created_at, updated_at)
             VALUES (?1, ?2, ?3, '!synthetic-no-login', 'active', ?4, ?5, ?6, 1, ?7, ?7)",
            (id.as_str(), format!("carga-{i:06}@load.invalid"), format!("Eleitor Sintético {i:06}"), m.as_str(), s.as_str(), c.as_str(), ts(now)),
        )
        .await?;
        tx.execute(
            "INSERT INTO user_roles (user_id, role, granted_at, granted_by) VALUES (?1, 'CITIZEN', ?2, 'seed-load')",
            (id.as_str(), ts(now)),
        )
        .await?;
        let token = session::create(&tx, now, policy, &id).await?;
        accounts.push((id, token));
    }
    tx.commit().await?;

    let keys = [flags::PROPOSALS, flags::SIGNATURES, flags::VOTING];
    for key in keys {
        flags::store(db, key, true, Some("seed-load")).await?;
    }
    let flags = flags::Flags::with_keys(&keys);
    let tz = config.country.timezone.parse().unwrap_or(chrono_tz::UTC);
    let ctx = Ctx {
        db,
        flags: &flags,
        now,
        clock: None,
        tz,
    };
    let (author_id, author_token) = &accounts[n];
    let author_hash = session::token_hash(author_token);
    let author = actor::load(&db.read().await?, author_id, &author_hash)
        .await?
        .ok_or_else(|| AppError::internal("missing or invalid stored value"))?;
    let publish = |kind: ProposalKind, title: String| {
        let (ctx, author, hash) = (&ctx, &author, author_hash.clone());
        async move {
            let input = DraftInput {
                kind,
                level: Level::National,
                summary: format!("Proposta sintética de carga: {}.", title.to_lowercase()),
                body: format!(
                    "Texto sintético para teste de carga, sem validade eleitoral.\n\nArt. 1º {title}."
                ),
                title,
                proposed_setup: None,
            };
            let id = proposals::save_draft(ctx, author, None, &input).await?.id;
            let tx = db.write().await?;
            reauth::grant(&tx, now, &hash, REAUTH_PUBLISH, &id).await?;
            tx.commit().await?;
            proposals::publish(ctx, author, &id, None).await?;
            Ok::<_, AppError>(id)
        }
    };
    for i in 1..=5 {
        publish(
            ProposalKind::NormalLaw,
            format!("[Carga] Lei em coleta {i}"),
        )
        .await?;
    }
    let id = publish(
        ProposalKind::UrgentLaw,
        "[Carga] Votação concentrada".into(),
    )
    .await?;
    let tx = db.write().await?;
    reauth::grant(&tx, now, &author_hash, REAUTH_SIGN, &id).await?;
    tx.commit().await?;
    let state = signatures::sign(&ctx, &author, &id, None).await?;
    if state.status != crate::domain::proposals::ProposalStatus::InVoting {
        return Err(AppError::internal(
            "seed-load proposal did not reach voting",
        ));
    }
    let tx = db.write().await?;
    for (_, token) in &accounts[..n] {
        reauth::grant(&tx, now, &session::token_hash(token), REAUTH_VOTE, &id).await?;
    }
    tx.commit().await?;
    Ok(serde_json::json!({
        "proposal_id": id,
        "cookie_name": config.session_cookie_name(),
        "tokens": accounts[..n].iter().map(|(_, t)| t.as_str()).collect::<Vec<_>>(),
    }))
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

    use crate::infrastructure::db::testing::migrated;

    fn config(env: &str, dir: &std::path::Path) -> Config {
        let db_path = dir.join("seed.db");
        let db_path = db_path.to_str().unwrap();
        let vars = [
            ("DWC_ENV", env),
            ("DWC_DB_PATH", db_path),
            ("DWC_COUNTRY", "BR"),
            ("DWC_IDENTITY_PROVIDER", "face-service"),
            ("DWC_FACE_SERVICE_URL", "http://127.0.0.1:8001"),
            ("DWC_FACE_SERVICE_KEY_FILE", "/nonexistent"),
            ("DWC_AUDIT_SIGNING_KEY_FILE", "/nonexistent"),
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
    async fn seed_load_is_test_only_and_prepares_eligible_voters() {
        let t = migrated().await;
        catalog::import(&t.db, "catalog/br/jurisdictions.csv")
            .await
            .unwrap();
        let dev = config("development", t.dir.path());
        assert_eq!(seed_load(&t.db, &dev, 3).await, Err(AppError::Forbidden));
        let test = config("test", t.dir.path());
        let data = seed_load(&t.db, &test, 3).await.unwrap();
        assert_eq!(data["tokens"].as_array().unwrap().len(), 3);
        let id = data["proposal_id"].as_str().unwrap();
        let conn = t.db.read().await.unwrap();
        let p = proposals::load(&conn, id).await.unwrap().unwrap();
        assert_eq!(p.status, crate::domain::proposals::ProposalStatus::InVoting);
        let (electorate, ..) = phases::vote_rules(&conn, &p).await.unwrap();
        assert_eq!(electorate, 4, "3 voters + author");
        let stored = flags::Flags::load(&t.db).await.unwrap();
        assert!(stored.is_enabled(flags::VOTING) && stored.is_enabled(flags::PROPOSALS));
        assert!(
            matches!(seed_load(&t.db, &test, 3).await, Err(AppError::Conflict(_))),
            "only on a database without setup"
        );
    }

    #[tokio::test]
    async fn seeds_synthetic_proposals_once_and_refuses_production() {
        let t = migrated().await;
        catalog::import(&t.db, "catalog/br/jurisdictions.csv")
            .await
            .unwrap();
        let mut dev = config("development", t.dir.path());
        dev.secret_key_file = None;
        assert_eq!(
            seed_dev(&t.db, &config("production", t.dir.path())).await,
            Err(AppError::Forbidden)
        );
        seed_dev(&t.db, &dev).await.unwrap();
        let again = seed_dev(&t.db, &dev).await.unwrap();
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
                "approved",
                "archived",
                "collecting_signatures",
                "collecting_signatures",
                "expired",
                "in_debate",
                "in_voting",
                "in_voting"
            ]
        );
        let results: i64 = query_opt(
            &conn,
            "SELECT yes_count * 100 + no_count * 10 + abstain_count FROM vote_results",
            (),
        )
        .await
        .unwrap()
        .unwrap()
        .get(0)
        .unwrap();
        assert_eq!(
            results, 311,
            "closed seeded vote: 3 SIM, 1 NÃO, 1 ABSTENÇÃO"
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
        let debate: (i64, i64) = {
            let row = query_opt(
                &conn,
                "SELECT (SELECT COUNT(*) FROM comments), (SELECT COUNT(*) FROM amendments)",
                (),
            )
            .await
            .unwrap()
            .unwrap();
            (row.get(0).unwrap(), row.get(1).unwrap())
        };
        assert_eq!(debate, (5, 1), "synthetic debate seeded once");
    }
}
