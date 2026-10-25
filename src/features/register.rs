//! Registration (`/cadastro`): name, e-mail, password and CEP. The address shown
//! comes from the server lookup; registration sends only the CEP and the server
//! persists its own lookup result. Stale lookup responses are ignored.

use leptos::{ev::SubmitEvent, prelude::*, task::spawn_local};
use serde::{Deserialize, Serialize};

use crate::{
    error::AppError,
    features::ui::{error_view, hard_navigate},
    i18n::t,
};

/// Address preview returned by the server (BrasilAPI field names).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddressView {
    pub cep: String,
    pub state: String,
    pub city: String,
    pub neighborhood: String,
    pub street: String,
    pub country: String,
}

#[server]
#[cfg_attr(
    feature = "ssr",
    tracing::instrument(level = "debug", skip_all, err(level = "debug"))
)]
pub async fn lookup_cep(cep: String) -> Result<AddressView, AppError> {
    use crate::{application::identity::register, features::face::bounded, server::context};
    let state = context::state()?;
    let deps = state.identity_deps();
    let (address, _) = register::lookup(
        &deps,
        state.identity.address.as_ref(),
        state.config.country.code,
        bounded(&cep, "cep", 16)?,
    )
    .await?;
    Ok(AddressView {
        cep: address.cep,
        state: address.state,
        city: address.city,
        neighborhood: address.neighborhood,
        street: address.street,
        country: state.config.country.name.to_owned(),
    })
}

#[server]
#[cfg_attr(
    feature = "ssr",
    tracing::instrument(level = "debug", skip_all, err(level = "debug"))
)]
pub async fn register_account(
    name: String,
    email: String,
    password: String,
    cep: String,
) -> Result<(), AppError> {
    use crate::{
        application::identity::register::{self, Registration},
        features::face::bounded,
        server::context,
    };
    let state = context::state()?;
    if let Ok(Some(_)) = context::current_actor(false).await {
        return Err(AppError::Conflict(t("auth.register.signed_in").into()));
    }
    let deps = state.identity_deps();
    let done = register::register(
        &deps,
        state.identity.address.as_ref(),
        state.config.country.code,
        Registration {
            name: bounded(&name, "name", 512)?,
            email: bounded(&email, "email", 320)?,
            password: bounded(&password, "password", 1024)?,
            cep: bounded(&cep, "cep", 16)?,
        },
    )
    .await?;
    context::set_session_cookie(&done.session_token);
    Ok(())
}

fn cep_digits(raw: &str) -> String {
    raw.chars().filter(char::is_ascii_digit).collect()
}

#[component]
pub fn RegisterPage() -> impl IntoView {
    let name = RwSignal::new(String::new());
    let email = RwSignal::new(String::new());
    let password = RwSignal::new(String::new());
    let cep = RwSignal::new(String::new());
    let address = RwSignal::new(None::<AddressView>);
    let seq = RwSignal::new(0u64);
    let looking = RwSignal::new(false);
    let lookup_error = RwSignal::new(None::<AppError>);
    let busy = RwSignal::new(false);
    let error = RwSignal::new(None::<AppError>);

    // Any CEP edit invalidates the previous result and in-flight lookups.
    Effect::new(move |_| {
        cep.track();
        seq.update(|s| *s += 1);
        address.set(None);
        lookup_error.set(None);
        looking.set(false);
    });

    let lookup = move |_| {
        let my = seq.get_untracked() + 1;
        seq.set(my);
        looking.set(true);
        lookup_error.set(None);
        let value = cep.get_untracked();
        spawn_local(async move {
            let result = lookup_cep(value).await;
            if seq.get_untracked() != my {
                return; // stale response for an older CEP
            }
            looking.set(false);
            match result {
                Ok(a) => address.set(Some(a)),
                Err(e) => lookup_error.set(Some(e)),
            }
        });
    };
    let confirmed = move || {
        address
            .get()
            .is_some_and(|a| a.cep == cep_digits(&cep.get()))
    };

    let submit = move |ev: SubmitEvent| {
        ev.prevent_default();
        if !confirmed() {
            return;
        }
        busy.set(true);
        error.set(None);
        let args = (
            name.get_untracked(),
            email.get_untracked(),
            password.get_untracked(),
            cep.get_untracked(),
        );
        spawn_local(async move {
            match register_account(args.0, args.1, args.2, args.3).await {
                Ok(()) => hard_navigate("/cadastro/biometria"),
                Err(e) => {
                    error.set(Some(e));
                    busy.set(false);
                }
            }
        });
    };

    view! {
        <section class="page narrow">
            <h1>{t("auth.register.title")}</h1>
            <form method="post" on:submit=submit novalidate=true>
                <label>{t("auth.field.name")}
                    <input type="text" name="name" autocomplete="name" required=true maxlength="120" data-testid="register-name" bind:value=name/>
                </label>
                <label>{t("auth.field.email")}
                    <input type="email" name="email" autocomplete="email" required=true maxlength="254" data-testid="register-email" bind:value=email/>
                </label>
                <label>{t("auth.field.password")}
                    <span class="hint">{t("auth.password.policy")}</span>
                    <input type="password" name="password" autocomplete="new-password" required=true minlength="12" maxlength="128" data-testid="register-password" bind:value=password/>
                </label>
                <label>{t("auth.field.cep")}
                    <input type="text" name="cep" inputmode="numeric" autocomplete="postal-code" required=true maxlength="9" data-testid="register-cep" bind:value=cep/>
                </label>
                <button type="button" class="secondary" data-testid="cep-lookup" on:click=lookup
                    disabled=move || looking.get() || cep_digits(&cep.get()).len() != 8>
                    {t("auth.cep.lookup")}
                </button>
                <Show when=move || looking.get()>
                    <p class="notice" aria-live="polite">{t("auth.cep.looking")}</p>
                </Show>
                {error_view(lookup_error)}
                {move || address.get().map(|a| view! {
                    <dl class="card address" data-testid="address-preview">
                        <dt>{t("profile.address.street")}</dt><dd>{or_dash(a.street)}</dd>
                        <dt>{t("profile.address.neighborhood")}</dt><dd>{or_dash(a.neighborhood)}</dd>
                        <dt>{t("profile.address.city")}</dt><dd data-testid="address-city">{format!("{} / {}", a.city, a.state)}</dd>
                        <dt>{t("profile.address.country")}</dt><dd>{a.country}</dd>
                    </dl>
                    <p class="hint">{t("auth.register.address_locked")}</p>
                })}
                <button type="submit" data-testid="register-submit" disabled=move || busy.get() || !confirmed()>
                    {t("auth.register.submit")}
                </button>
                {error_view(error)}
            </form>
            <p><a href="/login">{t("auth.register.have_account")}</a></p>
        </section>
    }
}

pub fn or_dash(value: String) -> String {
    if value.is_empty() {
        "—".into()
    } else {
        value
    }
}
