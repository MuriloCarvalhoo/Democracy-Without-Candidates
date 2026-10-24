//! Small shared view helpers (feedback states). Keep components local otherwise.

use leptos::prelude::*;

use crate::{error::AppError, i18n::t};

#[component]
pub fn ErrorNotice(error: AppError) -> impl IntoView {
    view! { <p class="notice error" role="alert">{error.user_message()}</p> }
}

#[component]
pub fn Loading() -> impl IntoView {
    view! { <p class="notice" aria-live="polite">{t("common.loading")}</p> }
}

#[component]
pub fn Empty(#[prop(optional)] message: Option<&'static str>) -> impl IntoView {
    view! { <p class="notice empty">{message.unwrap_or_else(|| t("common.empty"))}</p> }
}

/// Renders the error of a finished action, if any.
pub fn action_error<T: Clone + Send + Sync + 'static>(
    value: Option<Result<T, AppError>>,
) -> Option<AnyView> {
    match value {
        Some(Err(error)) => Some(view! { <ErrorNotice error/> }.into_any()),
        _ => None,
    }
}
