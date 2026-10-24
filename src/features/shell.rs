//! Layout, public instance information and connectivity state.

use leptos::prelude::*;
use leptos_router::components::A;
use serde::{Deserialize, Serialize};

use crate::{
    error::AppError,
    features::notifications::{NotificationsBadge, provide_notifications_refresh},
    i18n::t,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Viewer {
    pub name: String,
    pub pending: bool,
    pub is_admin: bool,
    pub is_auditor: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Instance {
    pub country: String,
    pub synthetic: bool,
    pub enabled: Vec<String>,
    pub viewer: Option<Viewer>,
}

impl Instance {
    pub fn enabled(&self, key: &str) -> bool {
        self.enabled.iter().any(|k| k == key)
    }
}

/// Public instance state. Does not renew session activity.
#[server]
pub async fn instance_info() -> Result<Instance, AppError> {
    use crate::{
        domain::identity::{AccountStatus, Role},
        server::context,
    };
    let state = context::state()?;
    let viewer = match context::current_actor(false).await {
        Ok(actor) => actor,
        Err(AppError::SessionExpired) => None,
        Err(e) => return Err(e),
    }
    .map(|a| Viewer {
        name: a.name.clone(),
        pending: a.status == AccountStatus::Pending,
        is_admin: a.has_role(Role::CountryAdmin),
        is_auditor: a.has_role(Role::Auditor),
    });
    Ok(Instance {
        country: state.config.country.name.to_owned(),
        synthetic: !state.config.is_production(),
        enabled: state
            .flags
            .enabled_keys()
            .into_iter()
            .map(String::from)
            .collect(),
        viewer,
    })
}

/// Instance info resource shared with pages through context.
pub type InstanceResource = Resource<Result<Instance, AppError>>;

#[component]
pub fn Layout(children: Children) -> impl IntoView {
    let info: InstanceResource = Resource::new(|| (), |_| instance_info());
    provide_context(info);
    provide_notifications_refresh();
    let online = RwSignal::new(true);
    // Effects only run in the browser: marks the page as hydrated for tests/E2E.
    let hydrated = RwSignal::new(false);
    Effect::new(move |_| hydrated.set(true));
    #[cfg(feature = "hydrate")]
    {
        let _ = window_event_listener(leptos::ev::offline, move |_| online.set(false));
        let _ = window_event_listener(leptos::ev::online, move |_| online.set(true));
    }

    let sign_out = move |_| {
        leptos::task::spawn_local(async move {
            let _ = crate::features::login::logout().await;
            crate::features::ui::hard_navigate("/");
        });
    };

    view! {
        <a class="skip" href="#conteudo">"Pular para o conteúdo"</a>
        <header class="site-header">
            <A href="/" attr:class="brand">{t("app.name")}</A>
            <nav aria-label=t("nav.main")>
                <A href="/">{t("nav.home")}</A>
                <A href="/propostas">{t("nav.proposals")}</A>
                <A href="/dashboard">{t("nav.dashboard")}</A>
                <A href="/setup/historico">{t("nav.setup_history")}</A>
                <A href="/auditoria">{t("nav.audit")}</A>
                <NotificationsBadge/>
                <Transition fallback=|| ()>
                    {move || info.get().map(|r| match r.ok().and_then(|i| i.viewer) {
                        None => view! {
                            <A href="/login" attr:data-testid="nav-login">{t("nav.login")}</A>
                            <A href="/cadastro" attr:data-testid="nav-register">{t("nav.register")}</A>
                        }.into_any(),
                        Some(v) => view! {
                            {v.pending.then(|| view! { <A href="/cadastro/biometria">{t("nav.finish")}</A> })}
                            <A href="/perfil" attr:data-testid="nav-profile">{t("nav.profile")}</A>
                            <button type="button" class="link" data-testid="nav-logout" on:click=sign_out>{t("nav.logout")}</button>
                        }.into_any(),
                    })}
                </Transition>
                <Transition fallback=|| ()>
                    {move || info.get().and_then(|r| r.ok()).and_then(|i| i.viewer).filter(|v| v.is_admin).map(|_| view! {
                        <A href="/admin">{t("nav.admin")}</A>
                    })}
                </Transition>
            </nav>
        </header>
        <crate::features::pwa::PwaRegistration/>
        <Transition fallback=|| ()>
            {move || info.get().and_then(|r| r.ok()).filter(|i| i.synthetic).map(|_| view! {
                <p class="banner" role="note">{t("app.synthetic_banner")}</p>
            })}
        </Transition>
        <Show when=move || !online.get()>
            <p class="banner offline" role="alert">{t("common.offline")}</p>
        </Show>
        <main id="conteudo" data-hydrated=move || hydrated.get().then_some("true")>{children()}</main>
    }
}
