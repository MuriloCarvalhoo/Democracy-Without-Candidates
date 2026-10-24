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
use leptos::prelude::*;
use leptos_axum::{LeptosRoutes, generate_route_list};

use super::AppState;
use crate::app::{App, shell};

pub fn router(state: AppState, leptos_options: LeptosOptions) -> Router {
    let routes = generate_route_list(App);
    let ctx_state = state.clone();
    Router::new()
        .route("/healthz", get(health))
        .leptos_routes_with_context(
            &leptos_options,
            routes,
            move || provide_context(ctx_state.clone()),
            {
                let leptos_options = leptos_options.clone();
                move || shell(leptos_options.clone())
            },
        )
        .fallback(leptos_axum::file_and_error_handler(shell))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            csrf_and_headers,
        ))
        .layer(tower_http::limit::RequestBodyLimitLayer::new(
            2 * 1024 * 1024,
        ))
        .with_state(leptos_options)
        .layer(axum::Extension(state))
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
