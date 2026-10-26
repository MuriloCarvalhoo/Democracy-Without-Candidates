//! Proposals, signatures, dashboard and setup history screens.

pub mod api;
mod detail;
mod form;

pub use detail::{ProposalPage, SignersPage};
pub(crate) use form::ProposalForm;
pub use form::{EditProposalPage, NewProposalPage};

use leptos::prelude::*;
use leptos_router::{
    components::{A, Form},
    hooks::use_query_map,
};

use crate::{
    domain::proposals::{
        Level, ProposalKind, ProposalStatus,
        view::{Place, ProposalPage as Page, ProposalSummary},
    },
    domain::setup::SetupParams,
    features::ui::{Empty, ErrorNotice, Loading},
    i18n::t,
};
use api::{dashboard_data, list_proposals, list_territories, setup_history};

/// Random client key for idempotent actions (one per user attempt).
pub(crate) fn new_idempotency_key() -> Option<String> {
    #[cfg(feature = "hydrate")]
    {
        let mut bytes = [0u8; 16];
        web_sys::window()?
            .crypto()
            .ok()?
            .get_random_values_with_u8_array(&mut bytes)
            .ok()?;
        Some(bytes.iter().map(|b| format!("{b:02x}")).collect())
    }
    #[cfg(not(feature = "hydrate"))]
    None
}

#[component]
pub(crate) fn StatusBadge(status: ProposalStatus) -> impl IntoView {
    view! {
        <span class=format!("badge status-{}", status.as_db()) data-testid="proposal-status">
            {t(status.label_key())}
        </span>
    }
}

#[component]
pub(crate) fn SignatureProgress(count: u64, target: u64) -> impl IntoView {
    view! {
        <div class="signatures">
            <progress class="progress" max=target.to_string() value=count.min(target).to_string()
                aria-label=t("proposals.signatures")></progress>
            <p data-testid="signature-count">
                {format!("{count} {} {target} {}", t("proposals.signatures.of"), t("proposals.signatures").to_lowercase())}
            </p>
        </div>
    }
}

#[component]
fn ProposalCard(p: ProposalSummary) -> impl IntoView {
    let href = format!("/propostas/{}", p.id);
    let collecting = p.status == ProposalStatus::CollectingSignatures;
    view! {
        <li class="card proposal-card" data-testid="proposal-item">
            <h3><A href=href>{p.title.clone()}</A></h3>
            <p class="row">
                <StatusBadge status=p.status/>
                <span class="badge">{t(p.kind.label_key())}</span>
                <span class="badge">{p.jurisdiction.clone()}</span>
            </p>
            <p>{p.summary.clone()}</p>
            {(collecting).then(|| p.signature_target.map(|target| view! { <SignatureProgress count=p.signature_count target/> }))}
            {p.deadline.clone().map(|d| view! { <p class="hint">{t("proposals.deadline")}": "{d}</p> })}
            {vote_links(&p.id, p.status)}
        </li>
    }
}

/// Vote/result links for proposals in or after voting.
pub(crate) fn vote_links(id: &str, status: ProposalStatus) -> Option<AnyView> {
    use ProposalStatus::*;
    match status {
        InVoting => Some(
            view! {
                <p class="row">
                    <A href=format!("/propostas/{id}/votar") attr:class="button" attr:data-testid="vote-link">{t("voting.vote_link")}</A>
                    <A href=format!("/propostas/{id}/resultado")>{t("voting.result.link")}</A>
                </p>
            }
            .into_any(),
        ),
        Approved | Rejected | QuorumNotReached => Some(
            view! {
                <p><A href=format!("/propostas/{id}/resultado") attr:data-testid="result-link">{t("voting.result.link")}</A></p>
            }
            .into_any(),
        ),
        _ => None,
    }
}

pub(crate) fn summary_list(items: Vec<ProposalSummary>, empty: &'static str) -> AnyView {
    if items.is_empty() {
        view! { <Empty message=empty/> }.into_any()
    } else {
        view! {
            <ul class="list proposal-list">
                {items.into_iter().map(|p| view! { <ProposalCard p/> }).collect_view()}
            </ul>
        }
        .into_any()
    }
}

fn select_options<T: Copy + Send + Sync + 'static>(
    all: &'static [T],
    value: fn(T) -> &'static str,
    label: fn(T) -> &'static str,
    current: Option<String>,
) -> impl IntoView {
    view! {
        <option value="" selected=current.is_none()>{t("proposals.filter.any")}</option>
        {all.iter().map(|v| {
            let v = *v;
            view! { <option value=value(v) selected=current.as_deref() == Some(value(v))>{t(label(v))}</option> }
        }).collect_view()}
    }
}

fn place_options(places: Vec<Place>, current: Option<String>) -> impl IntoView {
    view! {
        <option value="" selected=current.is_none()>{t("proposals.filter.any")}</option>
        {places.into_iter().map(|p| {
            let selected = current.as_deref() == Some(p.code.as_str());
            view! { <option value=p.code selected=selected>{p.name}</option> }
        }).collect_view()}
    }
}

fn page_href(q: &[(&str, Option<String>)], page: u32) -> String {
    let mut parts: Vec<String> = q
        .iter()
        .filter_map(|(k, v)| {
            v.as_ref()
                .filter(|v| !v.is_empty())
                .map(|v| format!("{k}={v}"))
        })
        .collect();
    parts.push(format!("page={page}"));
    format!("/propostas?{}", parts.join("&"))
}

/// `/propostas`: public list with type/status/level/state/city filters and
/// server pagination. Anyone may filter; signing and voting stay restricted
/// to the eligible electorate by their own use cases.
#[component]
pub fn ProposalsPage() -> impl IntoView {
    let query = use_query_map();
    let get = move |k: &str| query.read().get(k).filter(|v| !v.is_empty());
    let params = move || Filters {
        kind: get("kind"),
        status: get("status"),
        level: get("level"),
        state: get("state"),
        city: get("city"),
        page: get("page").and_then(|p| p.parse::<u32>().ok()),
    };
    let data = Resource::new(params, |f| {
        list_proposals(f.kind, f.status, f.level, f.state, f.city, f.page)
    });
    // State picked in the form before applying it; the URL wins after navigation.
    let picked = RwSignal::new(None::<Option<String>>);
    Effect::new(move |_| {
        query.track();
        picked.set(None);
    });
    let state = move || picked.get().unwrap_or_else(|| get("state"));
    let places = Resource::new(state, list_territories);
    let statuses: &'static [ProposalStatus] = &ProposalStatus::ALL[1..];

    view! {
        <section class="page">
            <div class="row spread">
                <h1>{t("proposals.list.title")}</h1>
                <A href="/propostas/nova" attr:class="button">{t("proposals.list.new")}</A>
            </div>
            <Form method="GET" action="/propostas">
                <div class="filters">
                    <label>{t("proposals.field.kind")}
                        <select name="kind" data-testid="filter-kind">
                            {move || select_options(ProposalKind::ALL, ProposalKind::as_db, ProposalKind::label_key, get("kind"))}
                        </select>
                    </label>
                    <label>{t("proposals.field.status")}
                        <select name="status" data-testid="filter-status">
                            {move || select_options(statuses, ProposalStatus::as_db, ProposalStatus::label_key, get("status"))}
                        </select>
                    </label>
                    <label>{t("proposals.field.level")}
                        <select name="level" data-testid="filter-level">
                            {move || select_options(Level::ALL, Level::as_db, Level::label_key, get("level"))}
                        </select>
                    </label>
                    <Transition fallback=|| ()>
                        {move || places.get().and_then(|r| r.ok()).map(|o| {
                            let same_state = state() == get("state");
                            let no_state = state().is_none();
                            view! {
                                <label>{t("proposals.field.country")}
                                    <select name="country" data-testid="filter-country">
                                        <option value=o.country.code selected>{o.country.name}</option>
                                    </select>
                                </label>
                                <label>{t("proposals.field.state")}
                                    <select name="state" data-testid="filter-state"
                                        on:change=move |ev| picked.set(Some(Some(event_target_value(&ev)).filter(|v| !v.is_empty())))>
                                        {place_options(o.states, state())}
                                    </select>
                                </label>
                                <label>{t("proposals.field.city")}
                                    <select name="city" data-testid="filter-city" disabled=no_state
                                        title=no_state.then(|| t("proposals.filter.pick_state"))>
                                        {place_options(o.cities, get("city").filter(|_| same_state))}
                                    </select>
                                </label>
                            }
                        })}
                    </Transition>
                </div>
                <div class="row">
                    <button type="submit" data-testid="filter-apply">{t("proposals.filter.apply")}</button>
                    <A href="/propostas" attr:class="button secondary">{t("proposals.filter.clear")}</A>
                </div>
            </Form>
            <Transition fallback=|| view! { <Loading/> }>
                {move || data.get().map(|r| match r {
                    Ok(page) => list_view(page, params()).into_any(),
                    Err(error) => view! { <ErrorNotice error/> }.into_any(),
                })}
            </Transition>
        </section>
    }
}

#[derive(Clone, PartialEq)]
struct Filters {
    kind: Option<String>,
    status: Option<String>,
    level: Option<String>,
    state: Option<String>,
    city: Option<String>,
    page: Option<u32>,
}

fn list_view(page: Page, f: Filters) -> impl IntoView {
    let q = [
        ("kind", f.kind),
        ("status", f.status),
        ("level", f.level),
        ("state", f.state),
        ("city", f.city),
    ];
    let filtered = q.iter().any(|(_, v)| v.is_some());
    let (prev, next) = (page.page > 1, page.page < page.pages);
    let empty = if filtered {
        "proposals.list.no_match"
    } else {
        "proposals.list.empty"
    };
    view! {
        <p class="hint" data-testid="proposal-total">{page.total}" "{t("proposals.list.total")}</p>
        {summary_list(page.items, t(empty))}
        <nav class="row" aria-label=t("proposals.page.of")>
            {prev.then(|| view! { <A href=page_href(&q, page.page - 1)>{t("proposals.page.prev")}</A> })}
            <span>{format!("{} {} / {}", t("proposals.page.of"), page.page, page.pages)}</span>
            {next.then(|| view! { <A href=page_href(&q, page.page + 1)>{t("proposals.page.next")}</A> })}
        </nav>
    }
}

/// `/dashboard`: active votes, debates and collections; personal lists for citizens.
#[component]
pub fn DashboardPage() -> impl IntoView {
    let data = Resource::new(|| (), |_| dashboard_data());
    view! {
        <section class="page">
            <h1>{t("dashboard.title")}</h1>
            <Transition fallback=|| view! { <Loading/> }>
                {move || data.get().map(|r| match r {
                    Ok(d) => view! {
                        <h2>{t("dashboard.voting")}</h2>
                        {summary_list(d.voting, t("common.empty"))}
                        <h2>{t("dashboard.debate")}</h2>
                        {summary_list(d.debate, t("common.empty"))}
                        <h2>{t("dashboard.collecting")}</h2>
                        {summary_list(d.collecting, t("common.empty"))}
                        {d.mine.map(|mine| view! { <h2>{t("dashboard.mine")}</h2>{summary_list(mine, t("common.empty"))} })}
                        {d.signed.map(|signed| view! { <h2>{t("dashboard.signed")}</h2>{summary_list(signed, t("common.empty"))} })}
                    }.into_any(),
                    Err(error) => view! { <ErrorNotice error/> }.into_any(),
                })}
            </Transition>
        </section>
    }
}

#[component]
pub(crate) fn SetupParamsTable(params: SetupParams) -> impl IntoView {
    view! {
        <table>
            <tbody>
                {params.fields().into_iter().map(|(name, value)| view! {
                    <tr><th scope="row">{t(name)}</th><td>{value}</td></tr>
                }).collect_view()}
            </tbody>
        </table>
    }
}

/// `/setup/historico`: public history of adopted setup versions.
#[component]
pub fn SetupHistoryPage() -> impl IntoView {
    let data = Resource::new(|| (), |_| setup_history());
    view! {
        <section class="page">
            <h1>{t("setup.history.title")}</h1>
            <p class="hint">{t("setup.history.inherit_note")}</p>
            <Transition fallback=|| view! { <Loading/> }>
                {move || data.get().map(|r| match r {
                    Ok(list) if list.is_empty() => view! { <Empty message=t("setup.history.empty")/> }.into_any(),
                    Ok(list) => view! {
                        <ul class="list">
                            {list.into_iter().map(|v| {
                                let status = if v.status == "adopted" { t("setup.status.adopted") } else { t("setup.status.superseded") };
                                view! {
                                    <li class="card" data-testid="setup-version">
                                        <h2>{v.jurisdiction.clone()}" — "{t("setup.history.version")}" "{v.version.unwrap_or_default()}</h2>
                                        <p class="row">
                                            <span class="badge">{status}</span>
                                            {v.synthetic.then(|| view! { <span class="badge">{t("setup.history.synthetic")}</span> })}
                                        </p>
                                        <p class="hint">{t("setup.history.adopted_at")}": "{v.adopted_at.clone().unwrap_or_default()}" (UTC)"</p>
                                        {v.source_proposal_id.clone().map(|id| view! {
                                            <p><A href=format!("/propostas/{id}")>{t("setup.history.source")}</A></p>
                                        })}
                                        <details><summary>{t("proposals.new.setup_params")}</summary><SetupParamsTable params=v.params/></details>
                                    </li>
                                }
                            }).collect_view()}
                        </ul>
                    }.into_any(),
                    Err(error) => view! { <ErrorNotice error/> }.into_any(),
                })}
            </Transition>
        </section>
    }
}
