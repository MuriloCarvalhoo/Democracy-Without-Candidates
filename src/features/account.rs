//! Signed-in credential management: password change (`/alterar-senha`) and new
//! recovery codes (`/perfil/codigos`). Both require password + face + TOTP.

use leptos::{ev::SubmitEvent, prelude::*, task::spawn_local};

use crate::{
    error::AppError,
    features::{
        face::{FaceCapture, FaceCaptured},
        ui::{RecoveryCodes, error_view, hard_navigate},
    },
    i18n::t,
};

#[server]
#[cfg_attr(
    feature = "ssr",
    tracing::instrument(level = "debug", skip_all, err(level = "debug"))
)]
pub async fn change_password(
    current_password: String,
    new_password: String,
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
    auth::change_password(
        &state.identity_deps(),
        &actor,
        bounded(&current_password, "password", 1024)?,
        bounded(&new_password, "password", 1024)?,
        FaceProof {
            challenge: bounded(&face_challenge, "face", 64)?,
            image: &image,
        },
        bounded(&totp_code, "totp", 16)?,
    )
    .await?;
    context::clear_session_cookie();
    Ok(())
}

#[server]
#[cfg_attr(
    feature = "ssr",
    tracing::instrument(level = "debug", skip_all, err(level = "debug"))
)]
pub async fn regenerate_codes(
    password: String,
    face_challenge: String,
    face_image_b64: String,
    totp_code: String,
) -> Result<Vec<String>, AppError> {
    use crate::{
        application::identity::{FaceProof, auth},
        features::face::{bounded, decode_image},
        server::context,
    };
    let state = context::state()?;
    let actor = context::require_actor().await?;
    let image = decode_image(&face_image_b64)?;
    auth::regenerate_recovery_codes(
        &state.identity_deps(),
        &actor,
        bounded(&password, "password", 1024)?,
        FaceProof {
            challenge: bounded(&face_challenge, "face", 64)?,
            image: &image,
        },
        bounded(&totp_code, "totp", 16)?,
    )
    .await
}

/// Face capture + TOTP input; the captured proof is single use, so it is cleared
/// after each submission.
#[component]
pub fn FaceAndTotp(face: RwSignal<Option<FaceCaptured>>, code: RwSignal<String>) -> impl IntoView {
    let on_capture = Callback::new(move |f| face.set(Some(f)));
    view! {
        <Show when=move || face.get().is_none() fallback=move || view! {
            <p class="notice success" data-testid="face-captured">{t("face.captured")}</p>
        }>
            <FaceCapture flow="session" on_capture/>
        </Show>
        <label>{t("auth.field.totp")}
            <input type="text" inputmode="numeric" autocomplete="one-time-code" maxlength="6" data-testid="factor-totp" bind:value=code/>
        </label>
    }
}

#[component]
pub fn ChangePasswordPage() -> impl IntoView {
    let current = RwSignal::new(String::new());
    let new_password = RwSignal::new(String::new());
    let face = RwSignal::new(None::<FaceCaptured>);
    let code = RwSignal::new(String::new());
    let busy = RwSignal::new(false);
    let error = RwSignal::new(None::<AppError>);
    let submit = move |ev: SubmitEvent| {
        ev.prevent_default();
        let Some(f) = face.get_untracked() else {
            error.set(Some(AppError::validation("face", t("face.required"))));
            return;
        };
        busy.set(true);
        error.set(None);
        let args = (
            current.get_untracked(),
            new_password.get_untracked(),
            code.get_untracked(),
        );
        spawn_local(async move {
            match change_password(args.0, args.1, f.challenge, f.image_b64, args.2).await {
                Ok(()) => hard_navigate("/login"),
                Err(e) => {
                    face.set(None);
                    error.set(Some(e));
                    busy.set(false);
                }
            }
        });
    };
    view! {
        <section class="page narrow">
            <h1>{t("account.password.title")}</h1>
            <p>{t("account.password.hint")}</p>
            <form method="post" on:submit=submit>
                <label>{t("auth.field.current_password")}
                    <input type="password" autocomplete="current-password" bind:value=current/>
                </label>
                <label>{t("auth.field.new_password")}
                    <span class="hint">{t("auth.password.policy")}</span>
                    <input type="password" autocomplete="new-password" minlength="12" maxlength="128" bind:value=new_password/>
                </label>
                <FaceAndTotp face code/>
                <button type="submit" disabled=move || busy.get()>{t("account.password.submit")}</button>
                {error_view(error)}
            </form>
        </section>
    }
}

#[component]
pub fn RecoveryCodesPage() -> impl IntoView {
    let password = RwSignal::new(String::new());
    let face = RwSignal::new(None::<FaceCaptured>);
    let code = RwSignal::new(String::new());
    let codes = RwSignal::new(None::<Vec<String>>);
    let busy = RwSignal::new(false);
    let error = RwSignal::new(None::<AppError>);
    let submit = move |ev: SubmitEvent| {
        ev.prevent_default();
        let Some(f) = face.get_untracked() else {
            error.set(Some(AppError::validation("face", t("face.required"))));
            return;
        };
        busy.set(true);
        error.set(None);
        let (p, c) = (password.get_untracked(), code.get_untracked());
        spawn_local(async move {
            match regenerate_codes(p, f.challenge, f.image_b64, c).await {
                Ok(new_codes) => codes.set(Some(new_codes)),
                Err(e) => error.set(Some(e)),
            }
            face.set(None);
            busy.set(false);
        });
    };
    view! {
        <section class="page narrow">
            <h1>{t("account.codes.title")}</h1>
            <Show when=move || codes.get().is_none() fallback=move || view! {
                <RecoveryCodes codes=codes.get().unwrap_or_default()/>
                <p><a href="/perfil">{t("nav.profile")}</a></p>
            }>
                <p>{t("account.codes.hint")}</p>
                <form method="post" on:submit=submit>
                    <label>{t("auth.field.password")}
                        <input type="password" autocomplete="current-password" bind:value=password/>
                    </label>
                    <FaceAndTotp face code/>
                    <button type="submit" disabled=move || busy.get()>{t("account.codes.submit")}</button>
                    {error_view(error)}
                </form>
            </Show>
        </section>
    }
}
