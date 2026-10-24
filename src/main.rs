#[cfg(feature = "ssr")]
#[tokio::main]
async fn main() -> std::process::ExitCode {
    use std::process::ExitCode;

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    match dwc::server::startup::run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            tracing::error!("startup failed: {error}");
            eprintln!("dwc: {error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(not(feature = "ssr"))]
pub fn main() {}
