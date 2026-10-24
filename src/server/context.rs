//! Helpers used inside server functions. Server functions are public network
//! endpoints: always resolve the caller here, never trust browser-provided state.

use axum::http::{HeaderMap, HeaderValue, header};
use leptos::prelude::*;
use leptos_axum::{ResponseOptions, extract};

use super::AppState;
use crate::{
    application::{actor::Actor, session},
    error::AppError,
};

pub fn state() -> Result<AppState, AppError> {
    use_context::<AppState>().ok_or(AppError::Internal)
}

fn cookie_value(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(k, _)| *k == name)
        .map(|(_, v)| v.to_owned())
}

/// Raw session token from the request cookie, if present.
pub async fn session_token() -> Result<Option<String>, AppError> {
    let state = state()?;
    let headers: HeaderMap = extract().await.map_err(|_| AppError::Internal)?;
    Ok(cookie_value(&headers, state.config.session_cookie_name()))
}

/// Current actor. `touch` renews the idle window (explicit navigation/actions only).
pub async fn current_actor(touch: bool) -> Result<Option<Actor>, AppError> {
    let state = state()?;
    let Some(token) = session_token().await? else {
        return Ok(None);
    };
    match session::resolve(
        &state.db,
        state.now(),
        state.session_policy(),
        &token,
        touch,
    )
    .await
    {
        Err(AppError::SessionExpired) => {
            clear_session_cookie();
            Err(AppError::SessionExpired)
        }
        other => other,
    }
}

/// Actor required for protected operations; renews activity.
pub async fn require_actor() -> Result<Actor, AppError> {
    current_actor(true).await?.ok_or(AppError::Unauthenticated)
}

/// Host-only session cookie: HttpOnly, SameSite=Lax, Path=/, no Domain/Max-Age/Expires
/// (browser-session cookie). Secure + `__Host-` prefix in production.
pub fn set_session_cookie(token: &str) {
    let Ok(state) = state() else { return };
    let secure = if state.config.secure_cookies {
        "; Secure"
    } else {
        ""
    };
    let cookie = format!(
        "{}={token}; HttpOnly; SameSite=Lax; Path=/{secure}",
        state.config.session_cookie_name()
    );
    if let (Some(response), Ok(value)) = (
        use_context::<ResponseOptions>(),
        HeaderValue::from_str(&cookie),
    ) {
        response.append_header(header::SET_COOKIE, value);
    }
}

pub fn clear_session_cookie() {
    let Ok(state) = state() else { return };
    let secure = if state.config.secure_cookies {
        "; Secure"
    } else {
        ""
    };
    let cookie = format!(
        "{}=; HttpOnly; SameSite=Lax; Path=/; Max-Age=0{secure}",
        state.config.session_cookie_name()
    );
    if let (Some(response), Ok(value)) = (
        use_context::<ResponseOptions>(),
        HeaderValue::from_str(&cookie),
    ) {
        response.append_header(header::SET_COOKIE, value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_named_cookie_only() {
        let mut h = HeaderMap::new();
        h.insert(
            header::COOKIE,
            HeaderValue::from_static("a=1; dwc_session=abc; dwc_session_x=no"),
        );
        assert_eq!(cookie_value(&h, "dwc_session").as_deref(), Some("abc"));
        assert_eq!(cookie_value(&h, "missing"), None);
    }
}
