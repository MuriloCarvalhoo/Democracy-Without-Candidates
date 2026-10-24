//! Process startup: validated configuration, migrations, catalog, flags, then serve
//! on loopback with graceful shutdown (stop accepting, drain up to 30 s).

use std::{path::Path, sync::Arc, time::Duration};

use leptos::prelude::get_configuration;

use super::{AppState, http::router};
use crate::{
    application::checkpoint::{self, CheckpointCheck},
    domain::audit::Checkpoint,
    infrastructure::{
        catalog,
        clock::SystemClock,
        config::{Config, IdentityProviderKind},
        db::Db,
        face::FaceVerifier,
        face_service::{FaceServiceClient, read_key_file},
        flags::Flags,
        migrations, signing,
    },
};

/// `dwc` serves; `dwc seed-dev` writes synthetic fixtures (non-production only);
/// `dwc audit-checkpoint <out.json>` signs the audit head for export outside the
/// VPS; `dwc audit-verify <checkpoint.json>` checks one against the local chain.
/// The audit commands open the database: run them while the service is stopped.
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
        Some("audit-checkpoint") => {
            let out = std::env::args()
                .nth(2)
                .ok_or("usage: dwc audit-checkpoint <out.json>")?;
            audit_checkpoint(&db, &config, std::path::Path::new(&out)).await
        }
        Some("audit-verify") => {
            let file = std::env::args()
                .nth(2)
                .ok_or("usage: dwc audit-verify <checkpoint.json>")?;
            audit_verify(&db, &config, std::path::Path::new(&file)).await
        }
        Some(other) => Err(format!(
            "unknown command {other}; use `serve`, `seed-dev`, `audit-checkpoint` or `audit-verify`"
        )),
    }
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
    let state = AppState {
        db,
        config: Arc::new(config),
        flags: Arc::new(flags),
        clock: Arc::new(SystemClock),
        face,
    };
    spawn_reminders(state.clone());
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

/// Delivers due scheduled notifications (deadline reminders) every minute.
fn spawn_reminders(state: AppState) {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(60));
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
