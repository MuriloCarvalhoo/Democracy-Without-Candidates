//! Process startup: validated configuration, migrations, catalog, flags, then serve
//! on loopback with graceful shutdown (stop accepting, drain up to 30 s).

use std::{sync::Arc, time::Duration};

use leptos::prelude::get_configuration;

use super::{AppState, http::router};
use crate::infrastructure::{
    catalog,
    clock::SystemClock,
    config::{Config, IdentityProviderKind},
    db::Db,
    face::FaceVerifier,
    face_service::{FaceServiceClient, read_key_file},
    flags::Flags,
    migrations,
};

/// `dwc` serves; `dwc seed-dev` writes synthetic fixtures (non-production only).
pub async fn run() -> Result<(), String> {
    let conf = get_configuration(None).map_err(|e| format!("leptos configuration: {e}"))?;
    let leptos_options = conf.leptos_options;
    let addr = leptos_options.site_addr;
    let config = Config::from_env(addr).map_err(|e| format!("configuration: {e}"))?;
    let db = prepare(&config).await?;

    match std::env::args().nth(1).as_deref() {
        None | Some("serve") => serve(db, config, leptos_options).await,
        Some("seed-dev") => {
            let done = crate::infrastructure::fixtures::seed_dev(&db, &config)
                .await
                .map_err(|e| format!("seed-dev: {e}"))?;
            for line in done {
                println!("{line}");
            }
            println!("restart the application to apply flag changes");
            Ok(())
        }
        Some(other) => Err(format!(
            "unknown command {other}; use `serve` or `seed-dev`"
        )),
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
