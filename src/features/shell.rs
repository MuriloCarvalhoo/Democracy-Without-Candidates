//! Layout, public instance information and connectivity state.

use leptos::prelude::*;
use leptos_router::components::A;
use serde::{Deserialize, Serialize};

use crate::{
    domain::iam::Permission,
    error::AppError,
    features::notifications::{NotificationsBadge, provide_notifications_refresh},
    i18n::t,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Viewer {
    pub name: String,
    pub pending: bool,
    /// Actions the viewer's IAM policies allow on every resource. Menus only:
    /// the server checks every request again.
    pub allowed: Vec<Permission>,
}

impl Viewer {
    pub fn can(&self, permission: Permission) -> bool {
        self.allowed.contains(&permission)
    }

    /// First administration page this viewer may open, if any.
    pub fn admin_home(&self) -> Option<&'static str> {
        [
            (Permission::AdminRead, "/admin"),
            (Permission::IamRead, "/admin/iam"),
            (Permission::ModerationAct, "/admin/moderacao"),
        ]
        .into_iter()
        .find_map(|(p, href)| self.can(p).then_some(href))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Instance {
    pub country: String,
    pub synthetic: bool,
    /// `DWC_DEV_SKIP_TOTP` is on (never in production).
    pub skip_totp: bool,
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
#[cfg_attr(
    feature = "ssr",
    tracing::instrument(level = "debug", skip_all, err(level = "debug"))
)]
pub async fn instance_info() -> Result<Instance, AppError> {
    use crate::{domain::identity::AccountStatus, server::context};
    let state = context::state()?;
    let viewer = match context::current_actor(false).await {
        Ok(actor) => actor,
        Err(AppError::SessionExpired) => None,
        Err(e) => return Err(e),
    }
    .map(|a| Viewer {
        name: a.name.clone(),
        pending: a.status == AccountStatus::Pending,
        allowed: a.access.allowed_actions(),
    });
    Ok(Instance {
        country: state.config.country.name.to_owned(),
        synthetic: !state.config.is_production(),
        skip_totp: state.config.dev_skip_totp,
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
            <A href="/" attr:class="brand">
                <img src="/icon.svg" alt="" width="32" height="32"/>
                <span>{t("app.name")}</span>
            </A>
            <nav class="primary-nav" aria-label=t("nav.main")>
                <A href="/">{t("nav.home")}</A>
                <A href="/propostas">{t("nav.proposals")}</A>
                <A href="/dashboard">{t("nav.dashboard")}</A>
                <A href="/metaleis">{t("nav.metalaws")}</A>
                <A href="/setup">{t("nav.setup_history")}</A>
                <A href="/estatisticas">{t("nav.statistics")}</A>
                <A href="/auditoria">{t("nav.audit")}</A>
                <Transition fallback=|| ()>
                    {move || info.get().and_then(|r| r.ok()).and_then(|i| i.viewer).and_then(|v| v.admin_home()).map(|href| view! {
                        <A href=href attr:data-testid="nav-admin">{t("nav.admin")}</A>
                    })}
                </Transition>
            </nav>
            <nav class="account-nav" aria-label=t("nav.account")>
                <NotificationsBadge/>
                <Transition fallback=|| ()>
                    {move || info.get().map(|r| match r.ok().and_then(|i| i.viewer) {
                        None => view! {
                            <A href="/login" attr:data-testid="nav-login">{t("nav.login")}</A>
                            <A href="/cadastro" attr:class="nav-cta" attr:data-testid="nav-register">{t("nav.register")}</A>
                        }.into_any(),
                        Some(v) => view! {
                            {v.pending.then(|| view! { <A href="/cadastro/biometria" attr:class="nav-cta">{t("nav.finish")}</A> })}
                            <A href="/perfil" attr:data-testid="nav-profile">{t("nav.profile")}</A>
                            <button type="button" class="link" data-testid="nav-logout" on:click=sign_out>{t("nav.logout")}</button>
                        }.into_any(),
                    })}
                </Transition>
            </nav>
        </header>
        <crate::features::pwa::PwaRegistration/>
        <Transition fallback=|| ()>
            {move || info.get().and_then(|r| r.ok()).filter(|i| i.synthetic).map(|i| view! {
                <p class="banner" role="note">{t("app.synthetic_banner")}</p>
                {i.skip_totp.then(|| view! {
                    <p class="banner security" role="note" data-testid="skip-totp-banner">{t("app.skip_totp_banner")}</p>
                })}
            })}
        </Transition>
        <Show when=move || !online.get()>
            <p class="banner offline" role="alert">{t("common.offline")}</p>
        </Show>
        <main id="conteudo" data-hydrated=move || hydrated.get().then_some("true")>{children()}</main>
        <footer class="site-footer">
            <div class="footer-inner">
                <p class="footer-brand">
                    <strong>{t("app.name")}</strong>
                    <Transition fallback=|| ()>
                        {move || info.get().and_then(|r| r.ok()).map(|i| view! {
                            <span class="hint">{t("footer.instance")}" "{i.country}</span>
                        })}
                    </Transition>
                </p>
                <p class="hint">{t("footer.online_only")}</p>
                <nav aria-label=t("footer.transparency")>
                    <A href="/auditoria">{t("footer.audit")}</A>
                    <A href="/estatisticas">{t("nav.statistics")}</A>
                    <A href="/setup/historico">{t("footer.setup_history")}</A>
                </nav>
            </div>
        </footer>
    }
}
