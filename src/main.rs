#[cfg(feature = "ssr")]
#[tokio::main]
async fn main() -> std::process::ExitCode {
    use std::process::ExitCode;

    dwc::server::observability::init();

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
