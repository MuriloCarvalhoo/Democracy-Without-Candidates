//! Logs reaching the terminal: per-environment default filter, request id and
//! access log, panics logged instead of dropped connections, `DWC_DEBUG` span
//! tracing and (development/test only) browser error reports.

use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use axum::{
    Router,
    body::{Body, Bytes},
    extract::DefaultBodyLimit,
    http::{Request, Response, StatusCode},
    routing::post,
};
use tower_http::{
    catch_panic::CatchPanicLayer,
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, RequestId, SetRequestIdLayer},
    trace::{DefaultOnResponse, TraceLayer},
};
use tracing::Level;
use tracing_subscriber::{EnvFilter, fmt::format::FmtSpan};

pub use crate::app::CLIENT_LOG_PATH;

/// Development/test default when `RUST_LOG` is unset: every application log.
pub const DEV_FILTER: &str = "dwc=info,tower_http=info,warn";
/// `DWC_DEBUG=1`: instrumented functions log entry (with safe inputs) and exit.
pub const DEBUG_FILTER: &str = "dwc=debug,tower_http=debug,info";
/// Production default: application info, everything else warnings only.
pub const PROD_FILTER: &str = "dwc=info,tower_http=warn,warn";

/// Largest accepted browser report body.
pub const CLIENT_LOG_MAX_BYTES: usize = 8 * 1024;
/// Browser reports accepted per minute (whole process); the rest get 429.
pub const CLIENT_LOG_PER_MINUTE: u32 = 60;

/// Default filter for `DWC_ENV`; unknown or missing environments are treated
/// as production. Debug applies only to development/test.
pub fn default_filter(env: Option<&str>, debug: bool) -> &'static str {
    match env {
        Some("development" | "test") if debug => DEBUG_FILTER,
        Some("development" | "test") => DEV_FILTER,
        _ => PROD_FILTER,
    }
}

/// True when `DWC_DEBUG` asks for debug tracing and the environment allows it.
/// Production refuses to start with `DWC_DEBUG` set (see `Config`).
fn debug_requested(env: Option<&str>, debug: Option<&str>) -> bool {
    matches!(debug.map(str::trim), Some("1" | "true"))
        && matches!(env, Some("development" | "test"))
}

/// Installs the process subscriber (before configuration is validated, so
/// startup errors are logged too) and the panic hook.
pub fn init() {
    let env = std::env::var("DWC_ENV").ok();
    let debug = debug_requested(env.as_deref(), std::env::var("DWC_DEBUG").ok().as_deref());
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(default_filter(env.as_deref(), debug)));
    // stderr: stdout stays clean for operator command output (JSON, paths).
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_span_events(span_events(debug))
        .with_writer(std::io::stderr)
        .with_ansi(std::io::IsTerminal::is_terminal(&std::io::stderr()))
        .init();
    install_panic_hook();
    if debug {
        tracing::warn!("DWC_DEBUG is on: instrumented calls are logged (development/test only)");
    }
}

/// Span entry (with its fields: the function inputs) and exit (with timing).
fn span_events(debug: bool) -> FmtSpan {
    if debug {
        FmtSpan::NEW | FmtSpan::CLOSE
    } else {
        FmtSpan::NONE
    }
}

/// Panics are logged through tracing (inside the current request span) before
/// the default hook prints them.
fn install_panic_hook() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        tracing::error!(panic = %info, "panic");
        default(info);
    }));
}

/// Request id (`x-request-id`, generated when absent, echoed in the response),
/// one access-log line per request (method, path, status, latency) and panic
/// capture: a panicking handler answers 500 instead of dropping the connection.
pub fn layer(router: Router) -> Router {
    router
        .layer(CatchPanicLayer::custom(panic_response))
        .layer(PropagateRequestIdLayer::x_request_id())
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(request_span)
                .on_request(())
                .on_response(DefaultOnResponse::new().level(Level::INFO))
                // Status already in the response line; causes are logged where
                // the error is created (`AppError::internal`).
                .on_failure(()),
        )
        .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
}

/// Path only: query strings may carry personal data.
fn request_span(request: &Request<Body>) -> tracing::Span {
    let id = request
        .extensions()
        .get::<RequestId>()
        .and_then(|id| id.header_value().to_str().ok())
        .unwrap_or("-");
    tracing::info_span!(
        "request",
        id = %id,
        method = %request.method(),
        path = %request.uri().path(),
    )
}

fn panic_response(_panic: Box<dyn std::any::Any + Send + 'static>) -> Response<Body> {
    tracing::error!("request handler panicked; answered 500");
    Response::builder()
        .status(StatusCode::INTERNAL_SERVER_ERROR)
        .header("content-type", "text/plain; charset=utf-8")
        .body(Body::from("internal error"))
        .unwrap_or_default()
}

/// Fixed-window limiter shared by the browser report route.
struct Window {
    started: Instant,
    count: u32,
}

#[derive(Clone)]
struct ClientLogLimit(Arc<Mutex<Window>>);

impl ClientLogLimit {
    fn new() -> Self {
        Self(Arc::new(Mutex::new(Window {
            started: Instant::now(),
            count: 0,
        })))
    }

    fn allow(&self) -> bool {
        let Ok(mut w) = self.0.lock() else {
            return false;
        };
        if w.started.elapsed() >= Duration::from_secs(60) {
            w.started = Instant::now();
            w.count = 0;
        }
        w.count += 1;
        w.count <= CLIENT_LOG_PER_MINUTE
    }
}

#[derive(serde::Deserialize)]
struct ClientReport {
    kind: String,
    message: String,
    #[serde(default)]
    url: String,
}

/// `POST /api/client-log` for development/test instances only: browser panics
/// and errors reach the server terminal. Not routed in production.
pub fn client_log_routes<S: Clone + Send + Sync + 'static>(production: bool) -> Router<S> {
    if production {
        return Router::new();
    }
    let limit = ClientLogLimit::new();
    Router::new().route(
        CLIENT_LOG_PATH,
        post(move |body: Bytes| client_log(limit.clone(), body))
            .layer(DefaultBodyLimit::max(CLIENT_LOG_MAX_BYTES)),
    )
}

async fn client_log(limit: ClientLogLimit, body: Bytes) -> StatusCode {
    if !limit.allow() {
        return StatusCode::TOO_MANY_REQUESTS;
    }
    let Ok(report) = serde_json::from_slice::<ClientReport>(&body) else {
        return StatusCode::BAD_REQUEST;
    };
    let kind = match report.kind.as_str() {
        k @ ("panic" | "error" | "rejection" | "app-error") => k,
        _ => "other",
    };
    let url = clean(&report.url, 256);
    let message = clean(&report.message, 4000);
    if kind == "app-error" {
        tracing::warn!(target: "dwc::client", kind, url = %url, "browser: {message}");
    } else {
        tracing::error!(target: "dwc::client", kind, url = %url, "browser: {message}");
    }
    StatusCode::NO_CONTENT
}

/// Bounded text without control characters other than newline and tab.
fn clean(text: &str, max_chars: usize) -> String {
    text.chars()
        .filter(|c| !c.is_control() || matches!(c, '\n' | '\t'))
        .take(max_chars)
        .collect()
}

#[cfg(test)]
pub mod testing {
    use std::{
        io::Write,
        sync::{Arc, Mutex},
    };

    use tracing_subscriber::fmt::MakeWriter;

    /// Captured log output of the current thread (debug level, span events on).
    #[derive(Clone, Default)]
    pub struct Captured(Arc<Mutex<Vec<u8>>>);

    impl Captured {
        pub fn text(&self) -> String {
            String::from_utf8_lossy(&self.0.lock().unwrap()).into_owned()
        }
    }

    impl Write for Captured {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl<'a> MakeWriter<'a> for Captured {
        type Writer = Self;

        fn make_writer(&'a self) -> Self {
            self.clone()
        }
    }

    /// Thread-local subscriber equivalent to `DWC_DEBUG=1` (keep the guard alive;
    /// use a current-thread runtime).
    pub fn capture(filter: &str) -> (tracing::subscriber::DefaultGuard, Captured) {
        let out = Captured::default();
        let subscriber = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::new(filter))
            .with_span_events(super::span_events(true))
            .with_ansi(false)
            .with_writer(out.clone())
            .finish();
        (tracing::subscriber::set_default(subscriber), out)
    }
}

#[cfg(test)]
mod tests {
    use axum::routing::get;
    use tower::ServiceExt;

    use super::*;

    #[test]
    fn default_filters_per_environment() {
        assert_eq!(default_filter(Some("development"), false), DEV_FILTER);
        assert_eq!(default_filter(Some("test"), false), DEV_FILTER);
        assert_eq!(default_filter(Some("development"), true), DEBUG_FILTER);
        assert_eq!(default_filter(Some("production"), false), PROD_FILTER);
        assert_eq!(default_filter(Some("production"), true), PROD_FILTER);
        assert_eq!(default_filter(None, true), PROD_FILTER);
        assert!(DEV_FILTER.contains("dwc=info") && DEV_FILTER.contains("tower_http=info"));
        assert!(!debug_requested(Some("production"), Some("1")));
        assert!(debug_requested(Some("development"), Some("1")));
        assert!(!debug_requested(Some("development"), Some("0")));
    }

    #[test]
    fn internal_error_logs_cause_and_call_site() {
        let (_guard, out) = testing::capture("dwc=debug");
        let _ = crate::error::AppError::internal("broken invariant xyz");
        let text = out.text();
        assert!(text.contains("broken invariant xyz"), "{text}");
        assert!(text.contains("observability.rs"), "call site: {text}");
    }

    #[tokio::test]
    async fn database_error_logs_question_mark_call_site() {
        let (_guard, out) = testing::capture("dwc=debug");
        fn fails() -> Result<(), crate::error::AppError> {
            Err(turso::Error::Misuse("boom".into()))?;
            Ok(())
        }
        assert_eq!(fails(), Err(crate::error::AppError::Internal));
        let text = out.text();
        assert!(
            text.contains("database error") && text.contains("boom"),
            "{text}"
        );
        assert!(
            text.contains("observability.rs"),
            "call site of `?`: {text}"
        );
    }

    async fn send(app: &Router, method: &str, path: &str, body: &str) -> Response<Body> {
        app.clone()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(path)
                    .body(Body::from(body.to_owned()))
                    .unwrap(),
            )
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn panicking_handler_answers_500_with_request_id_and_access_log() {
        let (_guard, out) = testing::capture("dwc=info,tower_http=info");
        let app = layer(
            Router::new()
                .route(
                    "/boom",
                    get(|| async {
                        let explode = true;
                        assert!(!explode, "handler exploded");
                        "unreachable"
                    }),
                )
                .route("/ok", get(|| async { "ok" })),
        );
        let res = send(&app, "GET", "/boom", "").await;
        assert_eq!(res.status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert!(res.headers().contains_key("x-request-id"));
        let res = send(&app, "GET", "/ok?secret=1", "").await;
        assert_eq!(res.status(), StatusCode::OK);
        let text = out.text();
        assert!(text.contains("handler panicked"), "{text}");
        assert!(
            text.contains("path=/ok") && text.contains("status=200"),
            "{text}"
        );
        assert!(
            !text.contains("secret=1"),
            "query strings are not logged: {text}"
        );
        assert!(text.contains("request{id="), "request id in span: {text}");
    }

    #[tokio::test]
    async fn client_log_is_dev_only_bounded_and_rate_limited() {
        let (_guard, out) = testing::capture("dwc=info");
        let prod: Router = client_log_routes(true);
        let res = send(
            &prod,
            "POST",
            CLIENT_LOG_PATH,
            r#"{"kind":"panic","message":"x"}"#,
        )
        .await;
        assert_eq!(
            res.status(),
            StatusCode::NOT_FOUND,
            "not routed in production"
        );

        let dev: Router = client_log_routes(false);
        let ok =
            r#"{"kind":"panic","message":"panicked at src/x.rs:1:1\u0007","url":"/notificacoes"}"#;
        assert_eq!(
            send(&dev, "POST", CLIENT_LOG_PATH, ok).await.status(),
            StatusCode::NO_CONTENT
        );
        let text = out.text();
        assert!(
            text.contains("browser: panicked at src/x.rs:1:1") && text.contains("/notificacoes"),
            "{text}"
        );
        assert!(!text.contains('\u{7}'), "control characters stripped");

        let big = format!(
            r#"{{"kind":"error","message":"{}"}}"#,
            "a".repeat(CLIENT_LOG_MAX_BYTES)
        );
        assert_eq!(
            send(&dev, "POST", CLIENT_LOG_PATH, &big).await.status(),
            StatusCode::PAYLOAD_TOO_LARGE
        );
        assert_eq!(
            send(&dev, "POST", CLIENT_LOG_PATH, "not json")
                .await
                .status(),
            StatusCode::BAD_REQUEST
        );

        let mut last = StatusCode::OK;
        for _ in 0..CLIENT_LOG_PER_MINUTE {
            last = send(&dev, "POST", CLIENT_LOG_PATH, ok).await.status();
        }
        assert_eq!(last, StatusCode::TOO_MANY_REQUESTS);
    }
}
