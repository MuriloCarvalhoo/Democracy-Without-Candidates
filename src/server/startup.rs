//! Process startup: validated configuration, migrations, catalog, flags, then serve
//! on loopback with graceful shutdown (stop accepting, drain up to 30 s).

use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use leptos::prelude::get_configuration;

use super::{AppState, IdentityServices, backup, http::router};
use crate::{
    application::checkpoint::{self, CheckpointCheck},
    domain::audit::Checkpoint,
    infrastructure::{
        address::AddressProvider,
        catalog,
        clock::SystemClock,
        config::{Config, IdentityProviderKind},
        db::Db,
        face::FaceVerifier,
        face_service::{FaceServiceClient, read_key_file},
        flags::Flags,
        migrations,
        secrets::SecretBox,
        signing,
    },
};

const USAGE: &str = "usage: dwc [serve | seed-dev | grant-admin <email> | backup <dest.db> | check-db | audit-checkpoint <out.json> | audit-verify <checkpoint.json> | seed-load <n> <out.json> | close-votes-at <rfc3339>]";

/// `dwc` serves. Operator commands run against a stopped instance because the
/// engine locks the database for the owning process:
/// `seed-dev` (synthetic fixtures, non-production), `grant-admin <email>` (bootstrap
/// COUNTRY_ADMIN for an existing active account), `backup <dest>` (consistent
/// snapshot), `check-db` (read-only verification, prints JSON), `audit-checkpoint
/// <out.json>` (signs the audit head for export outside the VPS) and
/// `audit-verify <checkpoint.json>` (checks one against the local chain).
/// Test-only (`DWC_ENV=test`): `seed-load <n> <out.json>` (synthetic voters with
/// sessions and reauth grants for load runs) and `close-votes-at <rfc3339>`
/// (closes votes due at that instant, to verify tallies after a load run).
pub async fn run() -> Result<(), String> {
    let conf = get_configuration(None).map_err(|e| format!("leptos configuration: {e}"))?;
    let mut leptos_options = conf.leptos_options;
    // cargo-leptos `hash-files` writes the hash file next to the binary; use hashed
    // asset names whenever it is there, so deployments cannot forget an env var.
    leptos_options.hash_files = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join(&*leptos_options.hash_file)))
        .is_some_and(|path| path.is_file());
    let addr = leptos_options.site_addr;
    let config = Config::from_env(addr).map_err(|e| format!("configuration: {e}"))?;
    let args: Vec<String> = std::env::args().skip(1).collect();

    match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        [] | ["serve"] => serve(prepare(&config).await?, config, leptos_options).await,
        ["seed-dev"] => {
            let db = prepare(&config).await?;
            let done = crate::infrastructure::fixtures::seed_dev(&db, &config)
                .await
                .map_err(|e| format!("seed-dev: {e}"))?;
            for line in done {
                println!("{line}");
            }
            println!("restart the application to apply flag changes");
            Ok(())
        }
        ["grant-admin", email] => {
            let db = prepare(&config).await?;
            crate::application::admin::grant_country_admin(&db, chrono::Utc::now(), email)
                .await
                .map_err(|e| format!("grant-admin: {}", e.user_message()))?;
            println!("COUNTRY_ADMIN granted (audited); effective on the account's next request");
            Ok(())
        }
        ["backup", dest] => {
            let db = open_existing(&config).await?;
            backup::snapshot(&db, Path::new(dest))
                .await
                .map_err(|e| format!("backup: {e}"))?;
            println!("snapshot written to {dest}");
            Ok(())
        }
        ["check-db"] => {
            let db = open_existing(&config).await?;
            let report = backup::check(&db)
                .await
                .map_err(|e| format!("check-db: {e}"))?;
            println!(
                "{}",
                serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?
            );
            if report.healthy() {
                Ok(())
            } else {
                Err("check-db: database is not healthy".into())
            }
        }
        ["audit-checkpoint", out] => {
            let db = open_existing(&config).await?;
            audit_checkpoint(&db, &config, Path::new(out)).await
        }
        ["seed-load", n, out] => {
            let out = Path::new(out);
            if out.exists() {
                return Err(format!("{} already exists", out.display()));
            }
            let n: usize = n.parse().map_err(|_| USAGE.to_owned())?;
            let db = prepare(&config).await?;
            let data = crate::infrastructure::fixtures::seed_load(&db, &config, n)
                .await
                .map_err(|e| format!("seed-load: {e}"))?;
            std::fs::write(out, data.to_string()).map_err(|e| format!("write: {e}"))?;
            println!(
                "{n} synthetic voters for proposal {} written to {}",
                data["proposal_id"],
                out.display()
            );
            Ok(())
        }
        ["close-votes-at", at] => {
            if config.env != crate::infrastructure::config::Environment::Test {
                return Err("close-votes-at: only with DWC_ENV=test".into());
            }
            let at = crate::infrastructure::clock::parse_ts(at).ok_or_else(|| USAGE.to_owned())?;
            let db = open_existing(&config).await?;
            let closed = crate::application::voting::close_due_votes(&db, at)
                .await
                .map_err(|e| format!("close-votes-at: {e}"))?;
            println!("{}", serde_json::json!({ "closed": closed }));
            Ok(())
        }
        ["audit-verify", file] => {
            let db = open_existing(&config).await?;
            audit_verify(&db, &config, Path::new(file)).await
        }
        _ => Err(USAGE.into()),
    }
}

/// Opens an existing database without migrating it (operator commands).
async fn open_existing(config: &Config) -> Result<Db, String> {
    if !config.db_path.is_file() {
        return Err(format!("database {} not found", config.db_path.display()));
    }
    Db::open(&config.db_path)
        .await
        .map_err(|e| format!("database {}: {e}", config.db_path.display()))
}

fn audit_key(config: &Config) -> Result<ed25519_dalek::SigningKey, String> {
    signing::load_or_create(&config.audit_signing_key_file, !config.is_production())
        .map_err(|e| e.to_string())
}

/// Signs the current audit head, records it and writes the JSON (never overwrites).
async fn audit_checkpoint(db: &Db, config: &Config, out: &Path) -> Result<(), String> {
    if out.exists() {
        return Err(format!("{} already exists", out.display()));
    }
    let key = audit_key(config)?;
    let cp = checkpoint::create(db, &key, config.country.code, chrono::Utc::now())
        .await
        .map_err(|e| format!("audit-checkpoint: {e}"))?;
    let json = serde_json::to_string_pretty(&cp).map_err(|e| e.to_string())?;
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(out)
        .and_then(|mut f| std::io::Write::write_all(&mut f, format!("{json}\n").as_bytes()))
        .map_err(|e| format!("write {}: {e}", out.display()))?;
    println!(
        "checkpoint seq={} head={} written to {}",
        cp.seq,
        cp.head_hash,
        out.display()
    );
    println!(
        "copy this file OUTSIDE the VPS; a local copy does not protect against a privileged operator"
    );
    Ok(())
}

/// Verifies signature, country and that the local chain still has the head at seq.
async fn audit_verify(db: &Db, config: &Config, file: &Path) -> Result<(), String> {
    let text =
        std::fs::read_to_string(file).map_err(|e| format!("read {}: {e}", file.display()))?;
    let cp: Checkpoint =
        serde_json::from_str(&text).map_err(|e| format!("invalid checkpoint JSON: {e}"))?;
    let local_key = signing::load_or_create(&config.audit_signing_key_file, false)
        .ok()
        .map(|k| signing::public_key_hex(&k));
    let conn = db.read().await.map_err(|e| e.to_string())?;
    let result = checkpoint::verify(&conn, &cp, config.country.code, local_key.as_deref())
        .await
        .map_err(|e| format!("audit-verify: {e}"))?;
    match result {
        CheckpointCheck::Valid { key_is_local } => {
            println!(
                "OK: signature valid and local chain has head {} at seq {}",
                cp.head_hash, cp.seq
            );
            if !key_is_local {
                println!(
                    "note: signed by {}, which is not the configured local key; confirm it through an external channel",
                    cp.public_key
                );
            }
            Ok(())
        }
        other => Err(format!("checkpoint verification FAILED: {other:?}")),
    }
}

/// Opens the database, applies migrations and imports the territorial catalog.
async fn prepare(config: &Config) -> Result<Db, String> {
    let db = Db::open(&config.db_path)
        .await
        .map_err(|e| format!("database {}: {e}", config.db_path.display()))?;
    let applied = migrations::run(&db)
        .await
        .map_err(|e| format!("migrations: {e}"))?;
    if !applied.is_empty() {
        tracing::info!(?applied, "migrations applied");
    }
    if catalog::import(&db, config.country.catalog_path)
        .await
        .map_err(|e| format!("catalog: {e}"))?
    {
        tracing::info!(
            catalog = config.country.catalog_path,
            "territorial catalog imported"
        );
    }
    Ok(db)
}

async fn serve(
    db: Db,
    config: Config,
    leptos_options: leptos::prelude::LeptosOptions,
) -> Result<(), String> {
    let addr = leptos_options.site_addr;
    let flags = Flags::load(&db)
        .await
        .map_err(|e| format!("feature flags: {e}"))?;
    tracing::info!(enabled = ?flags.enabled_keys(), env = ?config.env, country = config.country.code, "configuration loaded");
    if config.dev_skip_totp {
        tracing::warn!("DWC_DEV_SKIP_TOTP is on: TOTP codes are not checked (development only)");
    }

    // Fails in production when the audit signing key file is absent/unsafe.
    let audit_public_key = signing::public_key_hex(&audit_key(&config)?);
    tracing::info!(%audit_public_key, "audit checkpoint key loaded");

    let face = match (
        config.identity_provider,
        &config.face_service_url,
        &config.face_service_key_file,
    ) {
        (IdentityProviderKind::FaceService, Some(url), Some(key_file)) => {
            let key = read_key_file(key_file).map_err(|e| format!("face service key: {e}"))?;
            let client = FaceServiceClient::new(url, &key)
                .map_err(|e| format!("face service client: {e}"))?;
            match client.health().await {
                Ok(()) => tracing::info!(url = %client.base_url, "face service reachable"),
                Err(e) => {
                    tracing::warn!(url = %client.base_url, error = %e, "face service not reachable at startup")
                }
            }
            tracing::warn!(
                "face service: liveness is not validated server-side; a face match is not proof of a live person"
            );
            FaceVerifier::Service(client)
        }
        _ => FaceVerifier::Synthetic,
    };
    let address = AddressProvider::select(&flags, &config);
    if address.is_none() {
        tracing::warn!("no compatible address provider enabled; registration unavailable");
    }
    let identity = IdentityServices {
        secrets: SecretBox::load(&config)?,
        address,
    };
    spawn_snapshot_on_signal(db.clone(), snapshot_dir()?);
    let state = AppState {
        db,
        config: Arc::new(config),
        flags: Arc::new(flags),
        clock: Arc::new(SystemClock),
        face,
        identity: Arc::new(identity),
    };
    let ticker = Duration::from_secs(state.config.ticker_secs.unsigned_abs());
    spawn_reminders(state.clone(), ticker);
    // Deadline transitions resume from persisted instants after any restart.
    tokio::spawn(crate::application::phases::run_ticker(
        state.db.clone(),
        state.clock.clone(),
        ticker,
    ));
    let app = router(state, leptos_options);

    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .map_err(|e| format!("bind {addr}: {e}"))?;
    tracing::info!("listening on http://{addr}");
    println!("dwc listening on http://{addr}");

    let (stop_tx, stop_rx) = tokio::sync::oneshot::channel::<()>();
    let server = tokio::spawn(async move {
        // Peer address feeds the per-IP authentication limit.
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .with_graceful_shutdown(async {
            let _ = stop_rx.await;
        })
        .await
    });
    shutdown_signal().await;
    tracing::info!("shutdown requested; draining in-flight requests (max 30 s)");
    let _ = stop_tx.send(());
    match tokio::time::timeout(Duration::from_secs(30), server).await {
        Ok(Ok(Ok(()))) => Ok(()),
        Ok(Ok(Err(e))) => Err(format!("server: {e}")),
        Ok(Err(e)) => Err(format!("server task: {e}")),
        Err(_) => Err("graceful shutdown timed out after 30 s".into()),
    }
}

/// Delivers due scheduled notifications (vote opened/result, deadline
/// reminders) every ticker interval (30 s by default).
fn spawn_reminders(state: AppState, every: Duration) {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(every);
        loop {
            tick.tick().await;
            match crate::application::notify::remind_due(&state.db, state.now()).await {
                Ok(0) => {}
                Ok(n) => tracing::info!(delivered = n, "scheduled notifications delivered"),
                Err(e) => tracing::warn!(error = %e, "scheduled notifications failed"),
            }
        }
    });
}

/// Optional `DWC_SNAPSHOT_DIR`: existing absolute directory for online snapshots.
fn snapshot_dir() -> Result<Option<PathBuf>, String> {
    let Some(dir) = std::env::var_os("DWC_SNAPSHOT_DIR").map(PathBuf::from) else {
        return Ok(None);
    };
    if !dir.is_absolute() || !dir.is_dir() {
        return Err(format!(
            "DWC_SNAPSHOT_DIR {} must be an existing absolute directory",
            dir.display()
        ));
    }
    Ok(Some(dir))
}

/// Online backup: on SIGUSR1 the owning process writes `dwc-<UTC>.db` with the
/// engine's consistent copy. The handler is always installed so a stray signal
/// never terminates the server.
fn spawn_snapshot_on_signal(db: Db, dir: Option<PathBuf>) {
    #[cfg(unix)]
    tokio::spawn(async move {
        use tokio::signal::unix::{SignalKind, signal};
        let Ok(mut usr1) = signal(SignalKind::user_defined1()) else {
            tracing::error!("cannot install SIGUSR1 handler; online snapshots disabled");
            return;
        };
        while usr1.recv().await.is_some() {
            let Some(dir) = &dir else {
                tracing::warn!("snapshot requested but DWC_SNAPSHOT_DIR is not set");
                continue;
            };
            let name = format!("dwc-{}.db", chrono::Utc::now().format("%Y%m%dT%H%M%S%.3fZ"));
            let dest = dir.join(name);
            match backup::snapshot(&db, &dest).await {
                Ok(()) => tracing::info!(path = %dest.display(), "snapshot written"),
                Err(e) => tracing::error!(error = %e, "snapshot failed"),
            }
        }
    });
    #[cfg(not(unix))]
    let _ = (db, dir);
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut s) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            s.recv().await;
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}
