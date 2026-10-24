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
impl From<turso::Error> for AppError {
    fn from(error: turso::Error) -> Self {
        tracing::error!(%error, "database error");
        Self::Internal
    }
}

pub type AppResult<T> = Result<T, AppError>;
