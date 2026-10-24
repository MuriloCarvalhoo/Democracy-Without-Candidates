//! Reauthentication for critical actions (face + TOTP). Contract used by other
//! features: render `<ReauthPanel action resource on_granted/>`; after
//! `on_granted`, call the protected server function, which must
//! `application::reauth::consume(...)` inside its write transaction.
//! Placeholder until the identity feature lands.

use leptos::prelude::*;

use crate::error::AppError;

/// Verifies face + TOTP for the current session and records a grant bound to
/// `action` + `resource` (5 minutes, single use).
#[server]
pub async fn request_reauth(
    action: String,
    resource: String,
    face_image_b64: String,
    totp_code: String,
) -> Result<(), AppError> {
    let _ = (action, resource, face_image_b64, totp_code);
    Err(AppError::Unavailable(
        "reautenticação ainda não disponível".into(),
    ))
}

#[component]
pub fn ReauthPanel(action: String, resource: String, on_granted: Callback<()>) -> impl IntoView {
    let _ = (action, resource, on_granted);
    view! { <p class="notice">"Reautenticação indisponível."</p> }
}
