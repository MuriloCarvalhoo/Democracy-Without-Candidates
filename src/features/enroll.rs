//! Enrollment (`/cadastro/biometria`): face, then TOTP, then recovery codes shown
//! once. Also used for the forced TOTP re-enrollment after a recovery code login.

use leptos::{ev::SubmitEvent, prelude::*, task::spawn_local};
use serde::{Deserialize, Serialize};

use crate::{
    error::AppError,
    features::{
        face::{FaceCapture, FaceCaptured},
        ui::{ErrorNotice, RecoveryCodes, error_view, hard_navigate},
    },
    i18n::t,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnrollmentView {
    pub active: bool,
    pub face_enrolled: bool,
    pub totp_confirmed: bool,
    pub totp_reenroll_required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TotpSetup {
    pub secret_base32: String,
    pub otpauth_uri: String,
    pub qr_svg: String,
}

#[server]
pub async fn enrollment_status() -> Result<EnrollmentView, AppError> {
    use crate::{application::identity::enroll, domain::identity::AccountStatus, server::context};
    let state = context::state()?;
    let actor = context::require_actor().await?;
    let s = enroll::status(&state.identity_deps(), &actor.user_id).await?;
    Ok(EnrollmentView {
        active: actor.status == AccountStatus::Active,
        face_enrolled: s.face_enrolled,
        totp_confirmed: s.totp_confirmed,
        totp_reenroll_required: s.totp_reenroll_required,
    })
}

#[server]
pub async fn submit_face_enrollment(challenge: String, image_b64: String) -> Result<(), AppError> {
    use crate::{
        application::identity::{FaceProof, enroll},
        features::face::{bounded, decode_image},
        server::context,
    };
    let state = context::state()?;
    let actor = context::require_actor().await?;
    let image = decode_image(&image_b64)?;
    enroll::enroll_face(
        &state.identity_deps(),
        &actor,
        FaceProof {
            challenge: bounded(&challenge, "face", 64)?,
            image: &image,
        },
    )
    .await
}

#[server]
pub async fn start_totp_enrollment() -> Result<TotpSetup, AppError> {
    use crate::{application::identity::enroll, server::context};
    let state = context::state()?;
    let actor = context::require_actor().await?;
    let p = enroll::begin_totp(&state.identity_deps(), &actor).await?;
    Ok(TotpSetup {
        secret_base32: p.secret_base32,
        otpauth_uri: p.otpauth_uri,
        qr_svg: p.qr_svg,
    })
}

/// Returns the recovery codes on first enrollment (shown once).
#[server]
pub async fn confirm_totp_enrollment(code: String) -> Result<Option<Vec<String>>, AppError> {
    use crate::{application::identity::enroll, features::face::bounded, server::context};
    let state = context::state()?;
    let actor = context::require_actor().await?;
    let done =
        enroll::confirm_totp(&state.identity_deps(), &actor, bounded(&code, "totp", 16)?).await?;
    if let Some(token) = &done.session_token {
        context::set_session_cookie(token);
    }
    Ok(done.recovery_codes)
}

#[component]
pub fn EnrollPage() -> impl IntoView {
    let refresh = RwSignal::new(0u32);
    let status = Resource::new(move || refresh.get(), |_| enrollment_status());
    let codes = RwSignal::new(None::<Vec<String>>);
    let reenrolled = RwSignal::new(false);

    view! {
        <section class="page">
            <h1>{t("auth.enroll.title")}</h1>
            <Show when=move || codes.get().is_none() && !reenrolled.get()>
                <Transition fallback=|| view! { <p>{t("common.loading")}</p> }>
                    {move || status.get().map(|s| match s {
                        Err(error) => view! {
                            <ErrorNotice error/>
                            <p><a href="/login">{t("nav.login")}</a></p>
                        }.into_any(),
                        Ok(s) if !s.face_enrolled => view! {
                            <FaceStep on_done=Callback::new(move |_| refresh.update(|r| *r += 1))/>
                        }.into_any(),
                        Ok(s) if !s.totp_confirmed || s.totp_reenroll_required => view! {
                            <TotpStep on_done=Callback::new(move |c: Option<Vec<String>>| match c {
                                Some(c) => codes.set(Some(c)),
                                None => reenrolled.set(true),
                            })/>
                        }.into_any(),
                        Ok(_) => view! {
                            <p class="notice success">{t("auth.enroll.complete")}</p>
                            <p><a href="/perfil">{t("nav.profile")}</a></p>
                        }.into_any(),
                    })}
                </Transition>
            </Show>
            {move || codes.get().map(|codes| view! {
                <p class="notice success" role="status">{t("auth.enroll.activated")}</p>
                <RecoveryCodes codes/>
                <button type="button" data-testid="enroll-continue" on:click=|_| hard_navigate("/perfil")>
                    {t("auth.enroll.continue")}
                </button>
            })}
            <Show when=move || reenrolled.get()>
                <p class="notice success" role="status" data-testid="totp-reenrolled">{t("auth.enroll.reenrolled")}</p>
                <p><a href="/perfil">{t("nav.profile")}</a></p>
            </Show>
        </section>
    }
}

#[component]
fn FaceStep(on_done: Callback<()>) -> impl IntoView {
    let error = RwSignal::new(None::<AppError>);
    let busy = RwSignal::new(false);
    let on_capture = Callback::new(move |f: FaceCaptured| {
        busy.set(true);
        error.set(None);
        spawn_local(async move {
            match submit_face_enrollment(f.challenge, f.image_b64).await {
                Ok(()) => on_done.run(()),
                Err(e) => error.set(Some(e)),
            }
            busy.set(false);
        });
    });
    view! {
        <h2>{t("auth.enroll.face")}</h2>
        <p>{t("auth.enroll.face_hint")}</p>
        <FaceCapture flow="session" on_capture disabled=busy/>
        {error_view(error)}
    }
}

#[component]
fn TotpStep(on_done: Callback<Option<Vec<String>>>) -> impl IntoView {
    let setup = Resource::new(|| (), |_| start_totp_enrollment());
    let code = RwSignal::new(String::new());
    let error = RwSignal::new(None::<AppError>);
    let busy = RwSignal::new(false);
    let submit = move |ev: SubmitEvent| {
        ev.prevent_default();
        busy.set(true);
        error.set(None);
        let value = code.get_untracked();
        spawn_local(async move {
            match confirm_totp_enrollment(value).await {
                Ok(codes) => on_done.run(codes),
                Err(e) => error.set(Some(e)),
            }
            busy.set(false);
        });
    };
    view! {
        <h2>{t("auth.enroll.totp")}</h2>
        <p>{t("auth.enroll.totp_hint")}</p>
        <Suspense fallback=|| view! { <p>{t("common.loading")}</p> }>
            {move || setup.get().map(|s| match s {
                Ok(s) => view! {
                    <div class="qr" inner_html=s.qr_svg></div>
                    <p>{t("auth.enroll.totp_secret")}" "<code data-testid="totp-secret">{s.secret_base32}</code></p>
                    <p class="hint"><a href=s.otpauth_uri>{t("auth.enroll.totp_link")}</a></p>
                }.into_any(),
                Err(error) => view! { <ErrorNotice error/> }.into_any(),
            })}
        </Suspense>
        <form on:submit=submit>
            <label>{t("auth.field.totp")}
                <input type="text" inputmode="numeric" autocomplete="one-time-code" maxlength="6" data-testid="totp-code" bind:value=code/>
            </label>
            <button type="submit" data-testid="totp-confirm" disabled=move || busy.get()>{t("auth.enroll.totp_confirm")}</button>
            {error_view(error)}
        </form>
    }
}
