//! Transparency: public audit log with chain status and signed checkpoints, and
//! the restricted view for auditors/country admins (every access is audited).

use leptos::prelude::*;
use leptos_router::{components::A, hooks::use_query_map};

use crate::{
    domain::audit::{AuditListing, ChainSummary, EventView},
    error::AppError,
    features::{
        shell::InstanceResource,
        ui::{Empty, ErrorNotice, Loading},
    },
    i18n::t,
};

#[cfg(feature = "ssr")]
static CHAIN_CACHE: std::sync::LazyLock<crate::application::audit::ChainCache> =
    std::sync::LazyLock::new(Default::default);

/// Chain status, latest checkpoints and the checkpoint public key.
#[cfg(feature = "ssr")]
async fn listing(
    scope: crate::application::audit::Scope,
    events: Vec<EventView>,
    next_before: Option<i64>,
) -> Result<AuditListing, AppError> {
    use crate::{
        application::{audit, checkpoint},
        server::context,
    };
    static PUBLIC_KEY: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    let state = context::state()?;
    let conn = state.db.read().await?;
    let public_key = match PUBLIC_KEY.get() {
        Some(key) => Some(key.clone()),
        None => crate::infrastructure::signing::load_or_create(
            &state.config.audit_signing_key_file,
            false,
        )
        .ok()
        .map(|k| {
            PUBLIC_KEY
                .get_or_init(|| crate::infrastructure::signing::public_key_hex(&k))
                .clone()
        }),
    };
    let chain = audit::summary(&conn, &CHAIN_CACHE, state.now()).await?;
    Ok(AuditListing {
        events,
        next_before,
        kinds: chain.kinds(scope),
        chain: chain.status.into(),
        checkpoints: checkpoint::latest(&conn, state.config.country.code, 5).await?,
        public_key,
    })
}

/// Public events only, without actor references. Does not renew session activity.
#[server]
#[cfg_attr(feature = "ssr", tracing::instrument(level = "debug", skip_all, fields(kind = ?kind, before = ?before), err(level = "debug")))]
pub async fn public_audit_listing(
    kind: Option<String>,
    before: Option<i64>,
) -> Result<AuditListing, AppError> {
    use crate::{
        application::audit::{self, Scope},
        server::context,
    };
    let state = context::state()?;
    let kind = kind.filter(|k| !k.is_empty());
    let (events, next) = audit::list(
        &state.db.read().await?,
        Scope::Public,
        kind.as_deref(),
        before,
    )
    .await?;
    listing(Scope::Public, events, next).await
}

/// Requires `AuditReadRestricted`; the access is appended to the audit log first.
#[server]
#[cfg_attr(feature = "ssr", tracing::instrument(level = "debug", skip_all, fields(kind = ?kind, before = ?before), err(level = "debug")))]
pub async fn restricted_audit_listing(
    kind: Option<String>,
    before: Option<i64>,
) -> Result<AuditListing, AppError> {
    use crate::{
        application::audit::{self, Scope},
        server::context,
    };
    let actor = context::require_actor().await?;
    let state = context::state()?;
    let kind = kind.filter(|k| !k.is_empty());
    let (events, next) =
        audit::read_restricted(&state.db, state.now(), &actor, kind.as_deref(), before).await?;
    listing(Scope::Restricted, events, next).await
}

fn query_args(query: &leptos_router::params::ParamsMap) -> (Option<String>, Option<i64>) {
    (
        query.get("tipo").filter(|k| !k.is_empty()),
        query.get("antes").and_then(|b| b.parse().ok()),
    )
}

/// `2026-10-24T12:00:00.000Z` → `2026-10-24 12:00:00 UTC`.
pub fn format_instant(at: &str) -> String {
    format!("{} UTC", at.get(..19).unwrap_or(at).replace('T', " "))
}

#[component]
pub fn AuditPublicPage() -> impl IntoView {
    let query = use_query_map();
    let data = Resource::new(
        move || query_args(&query.get()),
        |(kind, before)| public_audit_listing(kind, before),
    );
    let info = use_context::<InstanceResource>();
    let can_read_restricted = move || {
        info.and_then(|i| i.get())
            .and_then(|r| r.ok())
            .and_then(|i| i.viewer)
            .is_some_and(|v| v.is_admin || v.is_auditor)
    };
    view! {
        <section class="page">
            <h1>{t("audit.title")}</h1>
            <p class="lead">{t("audit.lead")}</p>
            <Transition fallback=|| ()>
                <Show when=can_read_restricted>
                    <A href="/auditoria/avancada">{t("audit.advanced.link")}</A>
                </Show>
            </Transition>
        </section>
        <Transition fallback=|| view! { <Loading/> }>
            {move || data.get().map(|r| match r {
                Ok(listing) => view! { <ListingView listing base="/auditoria" restricted=false/> }.into_any(),
                Err(error) => view! { <ErrorNotice error/> }.into_any(),
            })}
        </Transition>
    }
}

#[component]
pub fn AuditAdvancedPage() -> impl IntoView {
    let query = use_query_map();
    let data = Resource::new(
        move || query_args(&query.get()),
        |(kind, before)| restricted_audit_listing(kind, before),
    );
    view! {
        <section class="page">
            <h1>{t("audit.advanced.title")}</h1>
            <p class="lead">{t("audit.advanced.lead")}</p>
        </section>
        <Transition fallback=|| view! { <Loading/> }>
            {move || data.get().map(|r| match r {
                Ok(listing) => view! { <ListingView listing base="/auditoria/avancada" restricted=true/> }.into_any(),
                Err(error) => view! {
                    <div data-testid="audit-denied"><ErrorNotice error/></div>
                    <A href="/auditoria">{t("audit.back_public")}</A>
                }.into_any(),
            })}
        </Transition>
    }
}

#[component]
fn ListingView(listing: AuditListing, base: &'static str, restricted: bool) -> impl IntoView {
    let AuditListing {
        events,
        next_before,
        kinds,
        chain,
        checkpoints,
        public_key,
    } = listing;
    let query = use_query_map();
    let current_kind = move || query.get().get("tipo").unwrap_or_default();
    let kind_suffix = move || {
        let k = current_kind();
        if k.is_empty() {
            String::new()
        } else {
            format!("&tipo={k}")
        }
    };
    let newest_href = move || {
        let k = current_kind();
        if k.is_empty() {
            base.to_owned()
        } else {
            format!("{base}?tipo={k}")
        }
    };
    let paged = move || query.get().get("antes").is_some();
    view! {
        <ChainCard chain/>
        <section class="card" aria-labelledby="audit-events-title">
            <h2 id="audit-events-title">{t("audit.events.title")}</h2>
            <form method="get" action=base class="row">
                <label>
                    {t("audit.filter.kind")}
                    <select name="tipo">
                        <option value="">{t("audit.filter.all")}</option>
                        {kinds.into_iter().map(|k| {
                            let (value, label) = (k.clone(), k.clone());
                            view! { <option value=k selected=move || value == current_kind()>{label}</option> }
                        }).collect_view()}
                    </select>
                </label>
                <button type="submit" class="secondary">{t("audit.filter.apply")}</button>
            </form>
            {if events.is_empty() {
                view! { <Empty message=t("audit.events.empty")/> }.into_any()
            } else {
                view! { <EventsTable events restricted/> }.into_any()
            }}
            <p class="row">
                <Show when=paged>
                    <A href=newest_href>{t("audit.newest")}</A>
                </Show>
                {next_before.map(|n| view! {
                    <A href=move || format!("{base}?antes={n}{}", kind_suffix()) attr:data-testid="audit-older">
                        {t("audit.older")}
                    </A>
                })}
            </p>
        </section>
        <section class="card" aria-labelledby="audit-checkpoints-title">
            <h2 id="audit-checkpoints-title">{t("audit.checkpoints.title")}</h2>
            <p>
                {t("audit.public_key")}": "
                <code data-testid="audit-public-key">
                    {public_key.unwrap_or_else(|| t("audit.public_key.none").to_owned())}
                </code>
            </p>
            {if checkpoints.is_empty() {
                view! { <Empty message=t("audit.checkpoints.none")/> }.into_any()
            } else {
                view! {
                    <table>
                        <thead><tr>
                            <th>{t("audit.col.seq")}</th>
                            <th>{t("audit.col.time")}</th>
                            <th>{t("audit.col.hash")}</th>
                        </tr></thead>
                        <tbody>
                            {checkpoints.into_iter().map(|c| view! {
                                <tr>
                                    <td>{c.seq}</td>
                                    <td>{format_instant(&c.created_at)}</td>
                                    <td><code title=c.signature.clone()>{c.head_hash.clone()}</code></td>
                                </tr>
                            }).collect_view()}
                        </tbody>
                    </table>
                }.into_any()
            }}
            <p class="hint">{t("audit.checkpoints.note")}</p>
        </section>
    }
}

#[component]
fn ChainCard(chain: ChainSummary) -> impl IntoView {
    let body = match chain {
        ChainSummary::Valid { events, head } => view! {
            <p class="notice success" data-testid="audit-chain-status">{t("audit.chain.valid")}</p>
            <p>{t("audit.chain.events")}": "<strong data-testid="audit-chain-events">{events}</strong></p>
            <p>{t("audit.chain.head")}": "<code data-testid="audit-chain-head">{head}</code></p>
        }
        .into_any(),
        ChainSummary::Broken { at_seq } => view! {
            <p class="notice error" role="alert" data-testid="audit-chain-status">
                {t("audit.chain.broken")}" "{at_seq}
            </p>
        }
        .into_any(),
    };
    view! {
        <section class="card" aria-labelledby="audit-chain-title">
            <h2 id="audit-chain-title">{t("audit.chain.title")}</h2>
            {body}
            <p class="hint">{t("audit.chain.note")}</p>
        </section>
    }
}

#[component]
fn EventsTable(events: Vec<EventView>, restricted: bool) -> impl IntoView {
    view! {
        <div class="table-scroll">
            <table data-testid="audit-events">
                <thead><tr>
                    <th>{t("audit.col.seq")}</th>
                    <th>{t("audit.col.time")}</th>
                    <th>{t("audit.col.kind")}</th>
                    <th>{t("audit.col.subject")}</th>
                    <th>{t("audit.col.payload")}</th>
                    {restricted.then(|| view! { <th>{t("audit.col.actor")}</th> })}
                    <th>{t("audit.col.hash")}</th>
                </tr></thead>
                <tbody>
                    {events.into_iter().map(|e| {
                        let short = e.hash.get(..12).unwrap_or(&e.hash).to_owned();
                        view! {
                            <tr data-kind=e.kind.clone()>
                                <td>{e.seq}</td>
                                <td>{format_instant(&e.occurred_at)}</td>
                                <td>
                                    {e.kind.clone()}
                                    {e.restricted.then(|| view! { " "<span class="badge">{t("audit.visibility.restricted")}</span> })}
                                </td>
                                <td>{e.subject.clone().unwrap_or_default()}</td>
                                <td><code>{e.payload.clone()}</code></td>
                                {restricted.then(|| view! { <td>{e.actor_ref.clone().unwrap_or_default()}</td> })}
                                <td><code title=e.hash.clone()>{short}</code></td>
                            </tr>
                        }
                    }).collect_view()}
                </tbody>
            </table>
        </div>
    }
}
