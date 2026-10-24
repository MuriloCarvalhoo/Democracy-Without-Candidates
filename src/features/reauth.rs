//! Reauthentication for critical actions (face + TOTP). Contract used by other
//! features: render `<ReauthPanel action resource on_granted/>`; after
//! `on_granted`, call the protected server function, which must
//! `application::reauth::consume(...)` inside its write transaction.

use leptos::{ev::SubmitEvent, prelude::*, task::spawn_local};

use crate::{
    error::AppError,
    features::{account::FaceAndTotp, face::FaceCaptured, ui::error_view},
    i18n::t,
};

/// Verifies face + TOTP for the current session and records a grant bound to
/// `action` + `resource` (5 minutes, single use). Failures are rate limited.
#[server]
pub async fn request_reauth(
    action: String,
    resource: String,
    face_challenge: String,
    face_image_b64: String,
    totp_code: String,
) -> Result<(), AppError> {
    use crate::{
        application::identity::{FaceProof, auth},
        features::face::{bounded, decode_image},
        server::context,
    };
    let state = context::state()?;
    let actor = context::require_actor().await?;
    let image = decode_image(&face_image_b64)?;
    auth::reauthenticate(
        &state.identity_deps(),
        &actor,
        context::client_ip().await,
        bounded(&action, "action", 64)?,
        bounded(&resource, "resource", 128)?,
        FaceProof {
            challenge: bounded(&face_challenge, "face", 64)?,
            image: &image,
        },
        bounded(&totp_code, "totp", 16)?,
    )
    .await
    .map(|_| ())
}

#[component]
pub fn ReauthPanel(action: String, resource: String, on_granted: Callback<()>) -> impl IntoView {
    let face = RwSignal::new(None::<FaceCaptured>);
    let code = RwSignal::new(String::new());
    let busy = RwSignal::new(false);
    let error = RwSignal::new(None::<AppError>);
    let target = StoredValue::new((action, resource));
    let submit = move |ev: SubmitEvent| {
        ev.prevent_default();
        let Some(f) = face.get_untracked() else {
            error.set(Some(AppError::validation("face", t("face.required"))));
            return;
        };
        busy.set(true);
        error.set(None);
        let (action, resource) = target.get_value();
        let totp = code.get_untracked();
        spawn_local(async move {
            let result = request_reauth(action, resource, f.challenge, f.image_b64, totp).await;
            face.set(None);
            code.set(String::new());
            busy.set(false);
            match result {
                Ok(()) => on_granted.run(()),
                Err(e) => error.set(Some(e)),
            }
        });
    };
    view! {
        <section class="card reauth" aria-labelledby="reauth-title">
            <h2 id="reauth-title">{t("reauth.title")}</h2>
            <p>{t("reauth.hint")}</p>
            <form on:submit=submit>
                <FaceAndTotp face code/>
                <button type="submit" data-testid="reauth-submit" disabled=move || busy.get()>{t("reauth.submit")}</button>
                {error_view(error)}
            </form>
        </section>
    }
}
