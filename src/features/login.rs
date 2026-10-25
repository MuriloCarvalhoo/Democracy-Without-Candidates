//! Login (`/login`): e-mail + password → face → TOTP or recovery code; logout;
//! password recovery (`/recuperar-senha`): e-mail → face → TOTP + new password,
//! without e-mail delivery. Step state lives in an HttpOnly cookie on the server.

use leptos::{ev::SubmitEvent, prelude::*, task::spawn_local};
use serde::{Deserialize, Serialize};

use crate::{
    error::AppError,
    features::{
        face::{FaceCapture, FaceCaptured},
        ui::{error_view, hard_navigate},
    },
    i18n::t,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LoginStep {
    Credentials,
    Face,
    Totp,
    /// Session created for a pending account: finish enrollment.
    Enroll,
    Done,
}

#[cfg(feature = "ssr")]
fn apply(outcome: crate::application::identity::auth::LoginOutcome) -> LoginStep {
    use crate::{application::identity::auth::LoginOutcome, server::context};
    match outcome {
        LoginOutcome::NeedFace => LoginStep::Face,
        LoginOutcome::NeedTotp => LoginStep::Totp,
        LoginOutcome::Session { token, pending } => {
            context::clear_auth_cookie();
            context::set_session_cookie(&token);
            if pending {
                LoginStep::Enroll
            } else {
                LoginStep::Done
            }
        }
    }
}

/// Revokes a previous session cookie, if any, before signing in again.
#[cfg(feature = "ssr")]
async fn drop_previous_session() -> Result<(), AppError> {
    use crate::{
        application::{identity::auth, session::token_hash},
        server::context,
    };
    if let Some(token) = context::session_token().await? {
        let state = context::state()?;
        auth::logout(&state.identity_deps(), &token_hash(&token)).await?;
    }
    Ok(())
}

#[server]
#[cfg_attr(
    feature = "ssr",
    tracing::instrument(level = "debug", skip_all, err(level = "debug"))
)]
pub async fn login_start(email: String, password: String) -> Result<LoginStep, AppError> {
    use crate::{application::identity::auth, features::face::bounded, server::context};
    let state = context::state()?;
    let deps = state.identity_deps();
    let (token, outcome) = auth::login_password(
        &deps,
        context::client_ip().await,
        bounded(&email, "email", 320)?,
        bounded(&password, "password", 1024)?,
    )
    .await?;
    if let Some(token) = token {
        context::set_auth_cookie(&token);
    } else {
        drop_previous_session().await?;
    }
    Ok(apply(outcome))
}

#[server]
#[cfg_attr(
    feature = "ssr",
    tracing::instrument(level = "debug", skip_all, err(level = "debug"))
)]
pub async fn login_face(challenge: String, image_b64: String) -> Result<LoginStep, AppError> {
    auth_face_step(challenge, image_b64, false).await
}

#[cfg(feature = "ssr")]
async fn auth_face_step(
    challenge: String,
    image_b64: String,
    reset: bool,
) -> Result<LoginStep, AppError> {
    use crate::{
        application::identity::{FaceProof, auth},
        features::face::{bounded, decode_image},
        server::context,
    };
    let state = context::state()?;
    let token = context::auth_token()
        .await?
        .ok_or_else(|| AppError::validation("flow", t("auth.error.flow_expired")))?;
    let image = decode_image(&image_b64)?;
    let purpose = if reset {
        auth::Purpose::PasswordReset
    } else {
        auth::Purpose::Login
    };
    let outcome = auth::auth_face(
        &state.identity_deps(),
        context::client_ip().await,
        &token,
        purpose,
        FaceProof {
            challenge: bounded(&challenge, "face", 64)?,
            image: &image,
        },
    )
    .await?;
    if matches!(outcome, auth::LoginOutcome::Session { .. }) {
        drop_previous_session().await?;
    }
    Ok(apply(outcome))
}

#[server]
#[cfg_attr(feature = "ssr", tracing::instrument(level = "debug", skip_all, fields(recovery = %recovery), err(level = "debug")))]
pub async fn login_finish(code: String, recovery: bool) -> Result<LoginStep, AppError> {
    use crate::{application::identity::auth, features::face::bounded, server::context};
    let state = context::state()?;
    let deps = state.identity_deps();
    let token = context::auth_token()
        .await?
        .ok_or_else(|| AppError::validation("flow", t("auth.error.flow_expired")))?;
    let ip = context::client_ip().await;
    let code = bounded(&code, "totp", 64)?;
    let session = if recovery {
        auth::login_recovery_code(&deps, ip, &token, code).await?
    } else {
        auth::login_totp(&deps, ip, &token, code).await?
    };
    drop_previous_session().await?;
    Ok(apply(auth::LoginOutcome::Session {
        token: session,
        pending: false,
    }))
}

#[server]
#[cfg_attr(
    feature = "ssr",
    tracing::instrument(level = "debug", skip_all, err(level = "debug"))
)]
pub async fn logout() -> Result<(), AppError> {
    use crate::{
        application::{identity::auth, session::token_hash},
        server::context,
    };
    let state = context::state()?;
    if let Some(token) = context::session_token().await? {
        auth::logout(&state.identity_deps(), &token_hash(&token)).await?;
    }
    context::clear_session_cookie();
    Ok(())
}

#[server]
#[cfg_attr(
    feature = "ssr",
    tracing::instrument(level = "debug", skip_all, err(level = "debug"))
)]
pub async fn reset_begin(email: String) -> Result<(), AppError> {
    use crate::{application::identity::auth, features::face::bounded, server::context};
    let state = context::state()?;
    let token = auth::reset_start(
        &state.identity_deps(),
        context::client_ip().await,
        bounded(&email, "email", 320)?,
    )
    .await?;
    context::set_auth_cookie(&token);
    Ok(())
}

#[server]
#[cfg_attr(
    feature = "ssr",
    tracing::instrument(level = "debug", skip_all, err(level = "debug"))
)]
pub async fn reset_face(challenge: String, image_b64: String) -> Result<LoginStep, AppError> {
    auth_face_step(challenge, image_b64, true).await
}

#[server]
#[cfg_attr(
    feature = "ssr",
    tracing::instrument(level = "debug", skip_all, err(level = "debug"))
)]
pub async fn reset_complete(code: String, new_password: String) -> Result<(), AppError> {
    use crate::{application::identity::auth, features::face::bounded, server::context};
    let state = context::state()?;
    let token = context::auth_token()
        .await?
        .ok_or_else(|| AppError::validation("flow", t("auth.error.flow_expired")))?;
    auth::reset_finish(
        &state.identity_deps(),
        context::client_ip().await,
        &token,
        bounded(&code, "totp", 16)?,
        bounded(&new_password, "password", 1024)?,
    )
    .await?;
    context::clear_auth_cookie();
    context::clear_session_cookie();
    Ok(())
}

fn after_login(step: LoginStep, current: RwSignal<LoginStep>) {
    match step {
        LoginStep::Done => hard_navigate("/"),
        LoginStep::Enroll => hard_navigate("/cadastro/biometria"),
        other => current.set(other),
    }
}

#[component]
pub fn LoginPage() -> impl IntoView {
    let step = RwSignal::new(LoginStep::Credentials);
    let email = RwSignal::new(String::new());
    let password = RwSignal::new(String::new());
    let code = RwSignal::new(String::new());
    let recovery = RwSignal::new(false);
    let busy = RwSignal::new(false);
    let error = RwSignal::new(None::<AppError>);

    let run = move |fut: std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<LoginStep, AppError>>>,
    >| {
        busy.set(true);
        error.set(None);
        spawn_local(async move {
            match fut.await {
                Ok(next) => after_login(next, step),
                Err(e) => error.set(Some(e)),
            }
            busy.set(false);
        });
    };
    let submit_credentials = move |ev: SubmitEvent| {
        ev.prevent_default();
        run(Box::pin(login_start(
            email.get_untracked(),
            password.get_untracked(),
        )));
    };
    let on_face =
        Callback::new(move |f: FaceCaptured| run(Box::pin(login_face(f.challenge, f.image_b64))));
    let submit_code = move |ev: SubmitEvent| {
        ev.prevent_default();
        run(Box::pin(login_finish(
            code.get_untracked(),
            recovery.get_untracked(),
        )));
    };

    view! {
        <section class="page narrow">
            <h1>{t("auth.login.title")}</h1>
            {move || match step.get() {
                LoginStep::Credentials => view! {
                    <form method="post" on:submit=submit_credentials>
                        <label>{t("auth.field.email")}
                            <input type="email" name="email" autocomplete="username" required=true data-testid="login-email" bind:value=email/>
                        </label>
                        <label>{t("auth.field.password")}
                            <input type="password" name="password" autocomplete="current-password" required=true data-testid="login-password" bind:value=password/>
                        </label>
                        <button type="submit" data-testid="login-submit" disabled=move || busy.get()>{t("auth.login.submit")}</button>
                    </form>
                    <p><a href="/recuperar-senha">{t("auth.login.forgot")}</a>" · "<a href="/cadastro">{t("nav.register")}</a></p>
                }.into_any(),
                LoginStep::Face => view! {
                    <h2>{t("auth.login.face")}</h2>
                    <FaceCapture flow="auth" on_capture=on_face disabled=busy/>
                }.into_any(),
                _ => view! {
                    <h2>{move || if recovery.get() { t("auth.login.recovery") } else { t("auth.login.totp") }}</h2>
                    <form method="post" on:submit=submit_code>
                        <label>{move || if recovery.get() { t("auth.field.recovery_code") } else { t("auth.field.totp") }}
                            <input type="text" autocomplete="one-time-code" data-testid="login-code" bind:value=code/>
                        </label>
                        <button type="submit" data-testid="login-code-submit" disabled=move || busy.get()>{t("auth.login.submit")}</button>
                    </form>
                    <button type="button" class="secondary" data-testid="login-toggle-recovery"
                        on:click=move |_| { recovery.update(|r| *r = !*r); code.set(String::new()); }>
                        {move || if recovery.get() { t("auth.login.use_totp") } else { t("auth.login.lost_totp") }}
                    </button>
                }.into_any(),
            }}
            {error_view(error)}
        </section>
    }
}

#[component]
pub fn ResetPasswordPage() -> impl IntoView {
    let step = RwSignal::new(LoginStep::Credentials);
    let email = RwSignal::new(String::new());
    let code = RwSignal::new(String::new());
    let new_password = RwSignal::new(String::new());
    let done = RwSignal::new(false);
    let busy = RwSignal::new(false);
    let error = RwSignal::new(None::<AppError>);

    let begin = move |ev: SubmitEvent| {
        ev.prevent_default();
        busy.set(true);
        error.set(None);
        let value = email.get_untracked();
        spawn_local(async move {
            match reset_begin(value).await {
                Ok(()) => step.set(LoginStep::Face),
                Err(e) => error.set(Some(e)),
            }
            busy.set(false);
        });
    };
    let on_face = Callback::new(move |f: FaceCaptured| {
        busy.set(true);
        error.set(None);
        spawn_local(async move {
            match reset_face(f.challenge, f.image_b64).await {
                Ok(next) => step.set(next),
                Err(e) => error.set(Some(e)),
            }
            busy.set(false);
        });
    });
    let finish = move |ev: SubmitEvent| {
        ev.prevent_default();
        busy.set(true);
        error.set(None);
        let (c, p) = (code.get_untracked(), new_password.get_untracked());
        spawn_local(async move {
            match reset_complete(c, p).await {
                Ok(()) => done.set(true),
                Err(e) => error.set(Some(e)),
            }
            busy.set(false);
        });
    };

    view! {
        <section class="page narrow">
            <h1>{t("auth.reset.title")}</h1>
            <Show when=move || !done.get() fallback=|| view! {
                <p class="notice success" role="status" data-testid="reset-done">{t("auth.reset.done")}</p>
                <p><a href="/login">{t("nav.login")}</a></p>
            }>
                {move || match step.get() {
                    LoginStep::Credentials => view! {
                        <p>{t("auth.reset.hint")}</p>
                        <form method="post" on:submit=begin>
                            <label>{t("auth.field.email")}
                                <input type="email" autocomplete="username" required=true data-testid="reset-email" bind:value=email/>
                            </label>
                            <button type="submit" disabled=move || busy.get()>{t("auth.reset.continue")}</button>
                        </form>
                    }.into_any(),
                    LoginStep::Face => view! {
                        <h2>{t("auth.login.face")}</h2>
                        <FaceCapture flow="auth" on_capture=on_face disabled=busy/>
                    }.into_any(),
                    _ => view! {
                        <form method="post" on:submit=finish>
                            <label>{t("auth.field.totp")}
                                <input type="text" inputmode="numeric" autocomplete="one-time-code" maxlength="6" data-testid="reset-code" bind:value=code/>
                            </label>
                            <label>{t("auth.field.new_password")}
                                <span class="hint">{t("auth.password.policy")}</span>
                                <input type="password" autocomplete="new-password" minlength="12" maxlength="128" data-testid="reset-password" bind:value=new_password/>
                            </label>
                            <button type="submit" disabled=move || busy.get()>{t("auth.reset.submit")}</button>
                        </form>
                    }.into_any(),
                }}
            </Show>
            {error_view(error)}
        </section>
    }
}
