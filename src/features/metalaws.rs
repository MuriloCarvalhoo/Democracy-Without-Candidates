//! Meta-laws (`setup_change`): listing, creation with a validated parameter form
//! and detail with current vs proposed parameters. They follow the proposal flow
//! (collection → debate → 30-day setup vote), reusing the proposal screens.

use leptos::prelude::*;
use leptos_router::{
    components::A,
    hooks::{use_params_map, use_query_map},
};

use crate::{
    domain::proposals::ProposalKind,
    features::{
        proposals::{
            ProposalForm, ProposalPage,
            api::{list_proposals, setup_defaults},
            summary_list,
        },
        ui::{ErrorNotice, Loading},
        voting::SetupDiffCard,
    },
    i18n::t,
};

/// `/metaleis`.
#[component]
pub fn MetaLawsPage() -> impl IntoView {
    let query = use_query_map();
    let page = move || query.read().get("page").and_then(|p| p.parse::<u32>().ok());
    let data = Resource::new(page, |page| {
        list_proposals(Some("setup_change".into()), None, None, page)
    });
    view! {
        <section class="page">
            <div class="row spread">
                <h1>{t("metalaws.title")}</h1>
                <A href="/metaleis/nova" attr:class="button">{t("metalaws.new")}</A>
            </div>
            <p class="hint">{t("metalaws.hint")}</p>
            <p><A href="/setup">{t("voting.setup.title")}</A>" · "<A href="/setup/historico">{t("setup.history.title")}</A></p>
            <Transition fallback=|| view! { <Loading/> }>
                {move || data.get().map(|r| match r {
                    Ok(p) => view! {
                        <p class="hint" data-testid="proposal-total">{p.total}" "{t("proposals.list.total")}</p>
                        {summary_list(p.items, t("metalaws.empty"))}
                    }.into_any(),
                    Err(error) => view! { <ErrorNotice error/> }.into_any(),
                })}
            </Transition>
        </section>
    }
}

/// `/metaleis/nova`: draft with the effective parameters prefilled; the server
/// validates each field (`SetupParams::validate`).
#[component]
pub fn NewMetaLawPage() -> impl IntoView {
    let defaults = Resource::new(|| (), |_| setup_defaults());
    view! {
        <section class="page">
            <h1>{t("metalaws.new")}</h1>
            <p class="hint">{t("metalaws.new_hint")}</p>
            <Transition fallback=|| view! { <Loading/> }>
                {move || defaults.get().map(|r| match r {
                    Ok(Some(params)) => view! {
                        <ProposalForm draft=None params fixed_kind=ProposalKind::SetupChange/>
                    }.into_any(),
                    Ok(None) => view! { <p class="notice" role="status">{t("proposals.error.no_setup")}</p> }.into_any(),
                    Err(error) => view! { <ErrorNotice error/> }.into_any(),
                })}
            </Transition>
        </section>
    }
}

/// `/metaleis/:id`: parameter diff plus the regular proposal detail and actions.
#[component]
pub fn MetaLawPage() -> impl IntoView {
    let params = use_params_map();
    let id = Memo::new(move |_| params.read().get("id").unwrap_or_default());
    view! {
        <section class="page">
            <SetupDiffCard id/>
        </section>
        <ProposalPage/>
    }
}
