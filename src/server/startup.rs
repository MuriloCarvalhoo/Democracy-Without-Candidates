//! Process startup: validated configuration, migrations, catalog, flags, then serve
//! on loopback with graceful shutdown (stop accepting, drain up to 30 s).

use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use leptos::prelude::get_configuration;

use super::{AppState, backup, http::router};
use crate::infrastructure::{
    catalog,
    clock::SystemClock,
    config::{Config, IdentityProviderKind},
    db::Db,
    face::FaceVerifier,
    face_service::FaceServiceClient,
    flags::Flags,
    migrations,
};

const USAGE: &str =
    "usage: dwc [serve | seed-dev | grant-admin <email> | backup <dest.db> | check-db]";

/// `dwc` serves. Operator commands run against a stopped instance because the
/// engine locks the database for the owning process:
/// `seed-dev` (synthetic fixtures, non-production), `grant-admin <email>` (bootstrap
/// COUNTRY_ADMIN for an existing active account), `backup <dest>` (consistent
/// snapshot) and `check-db` (read-only verification, prints JSON).
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

    let face = match (config.identity_provider, &config.face_service_url) {
        (IdentityProviderKind::FaceService, Some(url)) => {
            FaceVerifier::Service(FaceServiceClient::new(url))
        }
        _ => FaceVerifier::Synthetic,
    };
    spawn_snapshot_on_signal(db.clone(), snapshot_dir()?);
    let state = AppState {
        db,
        config: Arc::new(config),
        flags: Arc::new(flags),
        clock: Arc::new(SystemClock),
        face,
    };
    let app = router(state, leptos_options);

    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .map_err(|e| format!("bind {addr}: {e}"))?;
    tracing::info!("listening on http://{addr}");
    println!("dwc listening on http://{addr}");

    let (stop_tx, stop_rx) = tokio::sync::oneshot::channel::<()>();
    let server = tokio::spawn(async move {
        axum::serve(listener, app.into_make_service())
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
