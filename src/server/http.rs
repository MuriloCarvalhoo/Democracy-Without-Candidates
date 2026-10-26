//! Router assembly, CSRF origin verification, security headers and health check.

use std::time::Duration;

use axum::{
    Json, Router,
    body::Body,
    error_handling::HandleErrorLayer,
    extract::{Request, State},
    handler::Handler,
    http::{HeaderValue, Method, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::get,
};
use leptos::{prelude::*, server_fn::error::SERVER_FN_ERROR_HEADER};
use leptos_axum::{LeptosRoutes, generate_route_list};

use super::AppState;
use crate::{
    app::{App, shell},
    error::AppError,
};

pub fn router(state: AppState, leptos_options: LeptosOptions) -> Router {
    let routes = generate_route_list(App);
    let ctx_state = state.clone();
    let fallback_state = state.clone();
    // Site assets (`/pkg`, icons, manifest) are served without the reactive
    // runtime; anything else renders the 404 page, which needs the layout's
    // context. Leptos' own file handler looks up `ResponseOptions` through the
    // worker's last render owner (leptos #4946): an asset could get another
    // request's status or Set-Cookie, or the worker could deadlock.
    let not_found = leptos_axum::file_and_error_handler_with_context(
        move || provide_context(fallback_state.clone()),
        shell,
    )
    .with_state(leptos_options.clone());
    let assets = tower_http::services::ServeDir::new(&*leptos_options.site_root)
        .precompressed_br()
        .precompressed_gzip()
        .call_fallback_on_method_not_allowed(true)
        .fallback(not_found);
    let router = Router::new()
        .route("/healthz", get(health))
        .merge(super::observability::client_log_routes(
            state.config.is_production(),
        ))
        .route(
            "/propostas/{id}/exportar",
            get(crate::features::proposals::api::export_handler),
        )
        .leptos_routes_with_context(
            &leptos_options,
            routes,
            move || provide_context(ctx_state.clone()),
            {
                let leptos_options = leptos_options.clone();
                move || shell(leptos_options.clone())
            },
        )
        .fallback_service(assets)
        .layer(middleware::from_fn(server_fn_error_status))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            csrf_and_headers,
        ))
        .layer(tower_http::limit::RequestBodyLimitLayer::new(
            2 * 1024 * 1024,
        ))
        .with_state(leptos_options);
    let max_in_flight = state.config.max_in_flight;
    let router = protect(router.layer(axum::Extension(state)), max_in_flight);
    super::observability::layer(router)
}

/// Longest a handler may run before the request is answered 503. Above the
/// face service (10 s) and address lookup timeouts; equal to the shutdown drain.
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
/// Longest a client may take to send the request body (408 afterwards).
const BODY_TIMEOUT: Duration = Duration::from_secs(30);

/// Overload protection around every route and the fallback: at most
/// `max_in_flight` requests at once (one limit shared by all routes), the rest
/// shed with 503 instead of queueing without bound; handler and body timeouts.
/// A timed-out handler is dropped at an `.await`: an open write transaction is
/// rolled back on the next `Db::write()`.
pub fn protect(router: Router, max_in_flight: usize) -> Router {
    router
        .layer(tower_http::timeout::RequestBodyTimeoutLayer::new(
            BODY_TIMEOUT,
        ))
        .layer(tower_http::timeout::TimeoutLayer::with_status_code(
            StatusCode::SERVICE_UNAVAILABLE,
            REQUEST_TIMEOUT,
        ))
        .layer(
            tower::ServiceBuilder::new()
                .layer(HandleErrorLayer::new(|_: tower::BoxError| async {
                    (StatusCode::SERVICE_UNAVAILABLE, "server overloaded")
                }))
                .load_shed()
                .layer(tower::limit::GlobalConcurrencyLimitLayer::new(
                    max_in_flight,
                )),
        )
}

/// Readiness: database answers a query. No private details in the body.
async fn health(axum::Extension(state): axum::Extension<AppState>) -> Response {
    let ok = match state.db.read().await {
        Ok(conn) => conn.query("SELECT 1", ()).await.is_ok(),
        Err(_) => false,
    };
    let status = if ok {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    (
        status,
        Json(serde_json::json!({ "status": if ok { "ok" } else { "unavailable" } })),
    )
        .into_response()
}

/// Largest server function error body decoded to pick its status (errors are tiny).
const MAX_ERROR_BODY: usize = 64 * 1024;

/// `server_fn` answers every error with 500. Re-reads the JSON `AppError` body
/// (unchanged, the client still decodes it) and sets the status it maps to.
async fn server_fn_error_status(request: Request<Body>, next: Next) -> Response {
    let response = next.run(request).await;
    if response.status() != StatusCode::INTERNAL_SERVER_ERROR
        || !response.headers().contains_key(SERVER_FN_ERROR_HEADER)
    {
        return response;
    }
    let (mut parts, body) = response.into_parts();
    let Ok(bytes) = axum::body::to_bytes(body, MAX_ERROR_BODY).await else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };
    if let Ok(error) = serde_json::from_slice::<AppError>(&bytes) {
        parts.status =
            StatusCode::from_u16(error.http_status()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    }
    Response::from_parts(parts, Body::from(bytes))
}

/// Rejects state-changing requests whose Origin (or Sec-Fetch-Site) is not this
/// instance. Combined with SameSite=Lax cookies this is the CSRF defence for
/// server functions (OWASP origin verification).
async fn csrf_and_headers(
    State(state): State<AppState>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let safe = matches!(
        *request.method(),
        Method::GET | Method::HEAD | Method::OPTIONS
    );
    if !safe && !same_origin(&request, &state.config.public_origin) {
        return (StatusCode::FORBIDDEN, "cross-origin request rejected").into_response();
    }
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("same-origin"),
    );
    headers.insert(
        "permissions-policy",
        HeaderValue::from_static("camera=(self), microphone=(), geolocation=()"),
    );
    if state.config.secure_cookies {
        headers.insert(
            header::STRICT_TRANSPORT_SECURITY,
            HeaderValue::from_static("max-age=31536000"),
        );
    }
    response
}

fn same_origin(request: &Request<Body>, public_origin: &str) -> bool {
    let headers = request.headers();
    if let Some(origin) = headers.get(header::ORIGIN).and_then(|v| v.to_str().ok()) {
        return origin == public_origin;
    }
    // Browsers always send Origin on cross-site POST; fall back to fetch metadata.
    match headers.get("sec-fetch-site").and_then(|v| v.to_str().ok()) {
        Some("same-origin") | Some("none") => true,
        Some(_) => false,
        None => headers
            .get(header::REFERER)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|r| r == public_origin || r.starts_with(&format!("{public_origin}/"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(headers: &[(&str, &str)]) -> Request<Body> {
        let mut b = Request::builder().method(Method::POST).uri("/api/x");
        for (k, v) in headers {
            b = b.header(*k, *v);
        }
        b.body(Body::empty()).unwrap()
    }

    /// Runs `error` (or a plain 500 when `None`) through the status middleware.
    async fn status_through_layer(error: Option<AppError>) -> (StatusCode, String) {
        use leptos::server_fn::{error::FromServerFnError, response::Res};
        use tower::ServiceExt;
        let response = move || match &error {
            Some(error) => Response::error_response("/api/x", error.ser()),
            None => (StatusCode::INTERNAL_SERVER_ERROR, "\"Forbidden\"").into_response(),
        };
        let app = Router::new()
            .route(
                "/api/x",
                axum::routing::post(move || async move { response() }),
            )
            .layer(middleware::from_fn(server_fn_error_status));
        let res = app.oneshot(req(&[])).await.unwrap();
        let status = res.status();
        let body = axum::body::to_bytes(res.into_body(), MAX_ERROR_BODY)
            .await
            .unwrap();
        (status, String::from_utf8(body.to_vec()).unwrap())
    }

    #[tokio::test]
    async fn server_fn_errors_get_their_status_and_keep_the_body() {
        let cases = [
            (AppError::Forbidden, 403, "\"Forbidden\""),
            (AppError::NotFound, 404, "\"NotFound\""),
            (AppError::validation("title", "curto"), 422, "Validation"),
            (AppError::RateLimited, 429, "RateLimited"),
            (AppError::Internal, 500, "Internal"),
        ];
        for (error, status, body) in cases {
            let (got, text) = status_through_layer(Some(error)).await;
            assert_eq!(got.as_u16(), status, "{text}");
            assert!(text.contains(body), "{text}");
        }
        // Plain 500s without the server_fn marker are left alone.
        let (got, _) = status_through_layer(None).await;
        assert_eq!(got, StatusCode::INTERNAL_SERVER_ERROR);
    }

    /// Router over a disposable database and a site root holding `pkg/app.js`.
    async fn test_app() -> (Router, crate::infrastructure::db::testing::TestDb) {
        use crate::{
            infrastructure::{
                clock::SystemClock, config::Config, face::FaceVerifier, flags::Flags,
                secrets::SecretBox,
            },
            server::IdentityServices,
        };
        use std::sync::Arc;
        let t = crate::infrastructure::db::testing::migrated().await;
        let site = t.dir.path().join("site");
        std::fs::create_dir_all(site.join("pkg")).unwrap();
        std::fs::write(site.join("pkg/app.js"), "console.log(1)").unwrap();
        let config = Config::from_lookup("127.0.0.1:3000".parse().unwrap(), |k| {
            match k {
                "DWC_ENV" => Some("test"),
                "DWC_DB_PATH" => Some("unused.db"),
                "DWC_COUNTRY" => Some("BR"),
                "DWC_IDENTITY_PROVIDER" => Some("synthetic"),
                _ => None,
            }
            .map(String::from)
        })
        .unwrap();
        let state = AppState {
            db: t.db.clone(),
            config: Arc::new(config),
            flags: Arc::new(Flags::default()),
            clock: Arc::new(SystemClock),
            face: FaceVerifier::Synthetic,
            identity: Arc::new(IdentityServices {
                secrets: SecretBox::from_key(&[9; 32]),
                address: None,
            }),
        };
        let options = LeptosOptions::builder()
            .output_name("dwc")
            .site_root(site.to_string_lossy().into_owned())
            .build();
        (router(state, options), t)
    }

    /// Regression (leptos #4946): a finished render leaves its root owner current
    /// on the worker thread. A static file served afterwards on that thread must
    /// not pick up that request's `ResponseOptions` (status, Set-Cookie).
    #[tokio::test(flavor = "current_thread")]
    async fn static_files_do_not_inherit_another_requests_response_options() {
        use leptos::prelude::{Owner, provide_context};
        use leptos_axum::ResponseOptions;
        use tower::ServiceExt;
        let (app, _t) = test_app().await;
        let previous = Owner::new_root(None);
        previous.with(|| {
            let options = ResponseOptions::default();
            options.set_status(StatusCode::IM_A_TEAPOT);
            options.insert_header(
                header::SET_COOKIE,
                HeaderValue::from_static("dwc_session=; Max-Age=0"),
            );
            provide_context(options);
        });
        let res = app
            .oneshot(Request::get("/pkg/app.js").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        assert!(
            res.headers().get(header::SET_COOKIE).is_none(),
            "leaked Set-Cookie: {:?}",
            res.headers().get(header::SET_COOKIE)
        );
        drop(previous);
    }

    /// Missing assets and unknown pages still render the 404 page (any method).
    #[tokio::test]
    async fn unknown_paths_render_the_not_found_page() {
        use tower::ServiceExt;
        let (app, _t) = test_app().await;
        for (method, path) in [
            (Method::GET, "/pkg/missing.js"),
            (Method::GET, "/nao-existe"),
            (Method::POST, "/nao-existe"),
        ] {
            let res = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(method.clone())
                        .uri(path)
                        .header(header::ORIGIN, "http://127.0.0.1:3000")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(res.status(), StatusCode::NOT_FOUND, "{method} {path}");
            let body = axum::body::to_bytes(res.into_body(), 1 << 20)
                .await
                .unwrap();
            assert!(
                String::from_utf8_lossy(&body).contains("<html"),
                "{method} {path} renders the layout"
            );
        }
    }

    /// Slow handler behind the overload protection with `max` requests at once.
    fn slow_app(max: usize, delay: std::time::Duration) -> Router {
        protect(
            Router::new()
                .route(
                    "/slow",
                    get(move || async move {
                        tokio::time::sleep(delay).await;
                        "done"
                    }),
                )
                .route("/fast", get(|| async { "done" })),
            max,
        )
    }

    /// Beyond the in-flight limit (shared by all routes) the server answers 503
    /// at once (load shedding), and admits requests again once capacity frees up.
    #[tokio::test]
    async fn requests_beyond_the_limit_are_shed_with_503() {
        use tower::ServiceExt;
        let app = slow_app(1, std::time::Duration::from_millis(300));
        let get_slow = || Request::get("/slow").body(Body::empty()).unwrap();
        let first = tokio::spawn(app.clone().oneshot(get_slow()));
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let started = std::time::Instant::now();
        let shed = app
            .clone()
            .oneshot(Request::get("/fast").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(shed.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert!(
            started.elapsed() < std::time::Duration::from_millis(200),
            "not queued"
        );
        assert_eq!(first.await.unwrap().unwrap().status(), StatusCode::OK);
        assert_eq!(
            app.oneshot(get_slow()).await.unwrap().status(),
            StatusCode::OK
        );
    }

    /// A handler slower than the request timeout is cancelled with 503.
    #[tokio::test(start_paused = true)]
    async fn slow_handlers_time_out_with_503() {
        use tower::ServiceExt;
        let app = slow_app(8, REQUEST_TIMEOUT * 2);
        let res = app
            .oneshot(Request::get("/slow").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[test]
    fn origin_checks() {
        let o = "http://127.0.0.1:3000";
        assert!(same_origin(&req(&[("origin", o)]), o));
        assert!(!same_origin(&req(&[("origin", "https://evil.example")]), o));
        assert!(same_origin(&req(&[("sec-fetch-site", "same-origin")]), o));
        assert!(!same_origin(&req(&[("sec-fetch-site", "cross-site")]), o));
        assert!(same_origin(
            &req(&[("referer", "http://127.0.0.1:3000/propostas")]),
            o
        ));
        assert!(!same_origin(
            &req(&[("referer", "http://127.0.0.1:3000.evil/")]),
            o
        ));
        assert!(!same_origin(&req(&[]), o));
    }
}
