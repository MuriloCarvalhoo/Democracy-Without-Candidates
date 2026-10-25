//! Public error contract shared by server functions and the browser.

use serde::{Deserialize, Serialize};
use server_fn_codec::{FromServerFnError, JsonEncoding, ServerFnErrorErr};

use crate::i18n::t;

mod server_fn_codec {
    pub use leptos::server_fn::codec::JsonEncoding;
    pub use leptos::server_fn::error::{FromServerFnError, ServerFnErrorErr};
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
pub enum AppError {
    #[error("invalid field {field}: {message}")]
    Validation { field: String, message: String },
    #[error("authentication required")]
    Unauthenticated,
    #[error("session expired")]
    SessionExpired,
    #[error("forbidden")]
    Forbidden,
    #[error("not found")]
    NotFound,
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("too many attempts")]
    RateLimited,
    #[error("feature disabled: {0}")]
    FeatureDisabled(String),
    #[error("reauthentication required")]
    ReauthRequired,
    #[error("phase closed")]
    PhaseClosed,
    #[error("unavailable: {0}")]
    Unavailable(String),
    #[error("internal error")]
    Internal,
    #[error("transport error: {0}")]
    Transport(String),
}

impl AppError {
    pub fn validation(field: &str, message: impl Into<String>) -> Self {
        Self::Validation {
            field: field.to_owned(),
            message: message.into(),
        }
    }

    /// HTTP status for server function responses and SSR pages showing this error.
    /// Only `Internal` is a server fault (500); `Transport` on the server means the
    /// request could not be decoded (400).
    pub fn http_status(&self) -> u16 {
        match self {
            Self::Validation { .. } => 422,
            Self::Unauthenticated | Self::SessionExpired | Self::ReauthRequired => 401,
            Self::Forbidden | Self::FeatureDisabled(_) => 403,
            Self::NotFound => 404,
            Self::Conflict(_) | Self::PhaseClosed => 409,
            Self::RateLimited => 429,
            Self::Unavailable(_) => 503,
            Self::Transport(_) => 400,
            Self::Internal => 500,
        }
    }

    /// User-facing message in the instance language. Never contains private data.
    pub fn user_message(&self) -> String {
        match self {
            Self::Validation { message, .. } => message.clone(),
            Self::Unauthenticated => t("error.unauthenticated").into(),
            Self::SessionExpired => t("error.session_expired").into(),
            Self::Forbidden => t("error.forbidden").into(),
            Self::NotFound => t("error.not_found").into(),
            Self::Conflict(message) => message.clone(),
            Self::RateLimited => t("error.rate_limited").into(),
            Self::FeatureDisabled(_) => t("error.feature_disabled").into(),
            Self::ReauthRequired => t("error.reauth_required").into(),
            Self::PhaseClosed => t("error.phase_closed").into(),
            Self::Unavailable(message) => message.clone(),
            Self::Internal => t("error.internal").into(),
            Self::Transport(_) => t("error.transport").into(),
        }
    }
}

impl FromServerFnError for AppError {
    type Encoder = JsonEncoding;

    fn from_server_fn_error(value: ServerFnErrorErr) -> Self {
        Self::Transport(value.to_string())
    }
}

#[cfg(feature = "ssr")]
impl AppError {
    /// `Internal` with its cause and call site logged server-side (inside the
    /// request span, so with request id and path). The browser only sees the
    /// generic message. Never pass secrets or personal data as the cause.
    #[track_caller]
    pub fn internal(cause: impl std::fmt::Display) -> Self {
        let at = std::panic::Location::caller();
        tracing::error!(%cause, %at, "internal error");
        Self::Internal
    }
}

#[cfg(feature = "ssr")]
impl From<turso::Error> for AppError {
    /// Logged with the `?` call site. Lock contention that outlived the busy
    /// timeout becomes a retryable `Unavailable` instead of a generic failure.
    #[track_caller]
    fn from(error: turso::Error) -> Self {
        let at = std::panic::Location::caller();
        match error {
            turso::Error::Busy(_) | turso::Error::BusySnapshot(_) => {
                tracing::warn!(%error, %at, "database busy");
                Self::Unavailable(t("error.busy").into())
            }
            _ => {
                tracing::error!(%error, %at, "database error");
                Self::Internal
            }
        }
    }
}

pub type AppResult<T> = Result<T, AppError>;

#[cfg(test)]
mod tests {
    use super::AppError;

    #[test]
    fn http_status_matches_error_kind() {
        let cases = [
            (AppError::validation("f", "m"), 422),
            (AppError::Unauthenticated, 401),
            (AppError::SessionExpired, 401),
            (AppError::ReauthRequired, 401),
            (AppError::Forbidden, 403),
            (AppError::FeatureDisabled("x".into()), 403),
            (AppError::NotFound, 404),
            (AppError::Conflict("x".into()), 409),
            (AppError::PhaseClosed, 409),
            (AppError::RateLimited, 429),
            (AppError::Unavailable("x".into()), 503),
            (AppError::Transport("x".into()), 400),
            (AppError::Internal, 500),
        ];
        for (error, status) in cases {
            assert_eq!(error.http_status(), status, "{error:?}");
        }
    }
}
