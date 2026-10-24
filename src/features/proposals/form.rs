//! `/propostas/nova` and `/propostas/:id/editar` (author drafts).

use leptos::prelude::*;
use leptos_router::{
    components::A,
    hooks::{use_navigate, use_params_map},
};

use super::api::{SaveDraft, get_draft, setup_defaults};
use crate::{
    domain::{
        proposals::{
            BODY_MAX, Level, ProposalKind, SUMMARY_MAX, TITLE_MAX,
            view::{Draft, DraftInput},
        },
        setup::SetupParams,
    },
    features::ui::{ErrorNotice, Loading, action_error},
    i18n::t,
};

#[component]
pub fn NewProposalPage() -> impl IntoView {
    let defaults = Resource::new(|| (), |_| setup_defaults());
    view! {
        <section class="page">
            <h1>{t("proposals.new.title")}</h1>
            <Transition fallback=|| view! { <Loading/> }>
                {move || defaults.get().map(|r| match r {
                    Ok(Some(params)) => view! { <ProposalForm draft=None params/> }.into_any(),
                    Ok(None) => view! { <p class="notice" role="status">{t("proposals.error.no_setup")}</p> }.into_any(),
                    Err(error) => view! { <ErrorNotice error/> }.into_any(),
                })}
            </Transition>
        </section>
    }
}

#[component]
pub fn EditProposalPage() -> impl IntoView {
    let params = use_params_map();
    let id = Memo::new(move |_| params.read().get("id").unwrap_or_default());
    let data = Resource::new(
        move || id.get(),
        |id| async move {
            let draft = get_draft(id).await?;
            let defaults = setup_defaults().await?;
            Ok::<_, crate::error::AppError>((draft, defaults))
        },
    );
    view! {
        <section class="page">
            <h1>{t("proposals.edit.title")}</h1>
            <Transition fallback=|| view! { <Loading/> }>
                {move || data.get().map(|r| match r {
                    Ok((draft, defaults)) => {
                        let params = draft.proposed_setup.or(defaults).unwrap_or(crate::domain::setup::SUGGESTED);
                        view! { <ProposalForm draft=Some(draft) params/> }.into_any()
                    }
                    Err(error) => view! { <ErrorNotice error/> }.into_any(),
                })}
            </Transition>
        </section>
    }
}

#[component]
fn ProposalForm(draft: Option<Draft>, params: SetupParams) -> impl IntoView {
    let id = draft.as_ref().map(|d| d.id.clone());
    let kind = RwSignal::new(draft.as_ref().map_or(ProposalKind::NormalLaw, |d| d.kind));
    let level = RwSignal::new(draft.as_ref().map_or(Level::Municipal, |d| d.level));
    let title = RwSignal::new(draft.as_ref().map(|d| d.title.clone()).unwrap_or_default());
    let summary = RwSignal::new(
        draft
            .as_ref()
            .map(|d| d.summary.clone())
            .unwrap_or_default(),
    );
    let body = RwSignal::new(draft.as_ref().map(|d| d.body.clone()).unwrap_or_default());
    let values = RwSignal::new(params.fields().map(|(_, v)| v));
    let save = ServerAction::<SaveDraft>::new();
    let navigate = use_navigate();

    // Without duplicate warnings, go straight to the saved draft.
    Effect::new(move |_| {
        if let Some(Ok(saved)) = save.value().get()
            && saved.duplicates.is_empty()
        {
            navigate(&format!("/propostas/{}", saved.id), Default::default());
        }
    });

    let on_submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let kind = kind.get_untracked();
        save.dispatch(SaveDraft {
            id: save
                .value()
                .get_untracked()
                .and_then(|r| r.ok())
                .map(|s| s.id)
                .or_else(|| id.clone()),
            input: DraftInput {
                kind,
                level: level.get_untracked(),
                title: title.get_untracked(),
                summary: summary.get_untracked(),
                body: body.get_untracked(),
                proposed_setup: (kind == ProposalKind::SetupChange)
                    .then(|| SetupParams::from_values(values.get_untracked())),
            },
        });
    };
    let kinds = [
        ProposalKind::NormalLaw,
        ProposalKind::UrgentLaw,
        ProposalKind::SetupChange,
    ];

    view! {
        <p class="hint">{t("proposals.draft_notice")}</p>
        <form on:submit=on_submit>
            <label>{t("proposals.field.kind")}
                <select name="kind" on:change=move |ev| {
                    if let Some(k) = ProposalKind::from_db(&event_target_value(&ev)) { kind.set(k) }
                }>
                    {kinds.into_iter().map(|k| view! {
                        <option value=k.as_db() selected=move || kind.get() == k>{t(k.label_key())}</option>
                    }).collect_view()}
                </select>
            </label>
            <label>{t("proposals.field.level")}
                <span class="hint">{t("proposals.new.level_hint")}</span>
                <select name="level" on:change=move |ev| {
                    if let Some(l) = Level::from_db(&event_target_value(&ev)) { level.set(l) }
                }>
                    {Level::ALL.iter().map(|l| {
                        let l = *l;
                        view! { <option value=l.as_db() selected=move || level.get() == l>{t(l.label_key())}</option> }
                    }).collect_view()}
                </select>
            </label>
            <label>{t("proposals.field.title")}
                <input name="title" required minlength="5" maxlength=TITLE_MAX.to_string() prop:value=title
                    on:input=move |ev| title.set(event_target_value(&ev))/>
            </label>
            <label>{t("proposals.field.summary")}
                <textarea name="summary" required minlength="10" maxlength=SUMMARY_MAX.to_string() prop:value=summary
                    on:input=move |ev| summary.set(event_target_value(&ev))>{summary.get_untracked()}</textarea>
            </label>
            <label>{t("proposals.field.body")}
                <textarea name="body" required minlength="20" maxlength=BODY_MAX.to_string() rows="14" prop:value=body
                    on:input=move |ev| body.set(event_target_value(&ev))>{body.get_untracked()}</textarea>
            </label>
            <Show when=move || kind.get() == ProposalKind::SetupChange>
                <fieldset>
                    <legend>{t("proposals.new.setup_params")}</legend>
                    {params.fields().into_iter().enumerate().map(|(i, (name, _))| view! {
                        <label>{t(name)}
                            <input type="number" min="1" max="10000" required
                                prop:value=move || values.get()[i].to_string()
                                on:input=move |ev| {
                                    if let Ok(v) = event_target_value(&ev).parse::<u32>() {
                                        values.update(|vals| vals[i] = v);
                                    }
                                }/>
                        </label>
                    }).collect_view()}
                </fieldset>
            </Show>
            <button type="submit" data-testid="save-draft" disabled=move || save.pending().get()>
                {t("proposals.action.save")}
            </button>
        </form>
        {move || action_error(save.value().get())}
        {move || save.value().get().and_then(|r| r.ok()).filter(|s| !s.duplicates.is_empty()).map(|saved| view! {
            <div class="notice" role="status">
                <p>{t("proposals.saved")}" "{t("proposals.duplicates")}</p>
                <ul>
                    {saved.duplicates.into_iter().map(|d| view! {
                        <li><A href=format!("/propostas/{}", d.id)>{d.title}</A>" ("{t(d.status.label_key())}")"</li>
                    }).collect_view()}
                </ul>
                <A href=format!("/propostas/{}", saved.id) attr:class="button">{t("proposals.back")}</A>
            </div>
        })}
    }
}
