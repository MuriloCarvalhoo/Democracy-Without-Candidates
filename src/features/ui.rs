//! Small shared view helpers (feedback states). Keep components local otherwise.

use leptos::prelude::*;

use crate::{error::AppError, i18n::t};

/// During SSR, gives the document the HTTP status of the error it shows instead of
/// its content (no-op in the browser).
pub fn set_error_status(error: &AppError) {
    #[cfg(feature = "ssr")]
    if let Some(response) = use_context::<leptos_axum::ResponseOptions>() {
        use axum::http::StatusCode;
        response.set_status(
            StatusCode::from_u16(error.http_status()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
        );
    }
    #[cfg(not(feature = "ssr"))]
    let _ = error;
}

#[component]
pub fn ErrorNotice(error: AppError) -> impl IntoView {
    set_error_status(&error);
    crate::client_errors::report_app_error(&error);
    view! { <p class="notice error" role="alert">{error.user_message()}</p> }
}

#[component]
pub fn Loading() -> impl IntoView {
    view! { <p class="notice loading" aria-live="polite">{t("common.loading")}</p> }
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

/// Full page load: used after sign-in/out so the layout re-reads the session.
pub fn hard_navigate(path: &str) {
    #[cfg(feature = "hydrate")]
    {
        let _ = window().location().set_href(path);
    }
    #[cfg(not(feature = "hydrate"))]
    let _ = path;
}

/// Shows the recovery codes once, with guidance to store them.
#[component]
pub fn RecoveryCodes(codes: Vec<String>) -> impl IntoView {
    view! {
        <section class="card" aria-labelledby="codes-title">
            <h2 id="codes-title">{t("auth.codes.title")}</h2>
            <p>{t("auth.codes.hint")}</p>
            <ol class="codes" data-testid="recovery-codes">
                {codes.into_iter().map(|c| view! { <li><code>{c}</code></li> }).collect_view()}
            </ol>
        </section>
    }
}

/// Error from a signal, rendered when present.
pub fn error_view(error: RwSignal<Option<AppError>>) -> impl IntoView {
    move || error.get().map(|error| view! { <ErrorNotice error/> })
}
