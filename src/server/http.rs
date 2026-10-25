//! Router assembly, CSRF origin verification, security headers and health check.

use axum::{
    Json, Router,
    body::Body,
    extract::{Request, State},
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
        // The 404 page renders the same layout, so it needs the same context.
        .fallback(leptos_axum::file_and_error_handler_with_context(
            move || provide_context(fallback_state.clone()),
            shell,
        ))
        .layer(middleware::from_fn(server_fn_error_status))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            csrf_and_headers,
        ))
        .layer(tower_http::limit::RequestBodyLimitLayer::new(
            2 * 1024 * 1024,
        ))
        .with_state(leptos_options)
        .layer(axum::Extension(state));
    super::observability::layer(router)
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
