//! `/propostas/:id` and `/propostas/:id/assinantes`.

use leptos::prelude::*;
use leptos_router::{components::A, hooks::use_params_map};

use super::{
    SetupParamsTable, SignatureProgress, StatusBadge,
    api::{
        ArchiveProposal, PublishProposal, SignProposal, UnsignProposal, WithdrawProposal,
        get_proposal, proposal_signers,
    },
    new_idempotency_key,
};
use crate::{
    domain::proposals::{ProposalStatus, REAUTH_PUBLISH, REAUTH_SIGN, view::ProposalDetail},
    features::{
        reauth::ReauthPanel,
        ui::{ErrorNotice, Loading, action_error},
    },
    i18n::t,
};

#[derive(Clone, Copy)]
struct Actions {
    sign: ServerAction<SignProposal>,
    unsign: ServerAction<UnsignProposal>,
    publish: ServerAction<PublishProposal>,
    withdraw: ServerAction<WithdrawProposal>,
    archive: ServerAction<ArchiveProposal>,
    reauth_for: RwSignal<Option<&'static str>>,
}

fn proposal_id() -> Memo<String> {
    let params = use_params_map();
    Memo::new(move |_| params.read().get("id").unwrap_or_default())
}

#[component]
pub fn ProposalPage() -> impl IntoView {
    let id = proposal_id();
    let a = Actions {
        sign: ServerAction::new(),
        unsign: ServerAction::new(),
        publish: ServerAction::new(),
        withdraw: ServerAction::new(),
        archive: ServerAction::new(),
        reauth_for: RwSignal::new(None),
    };
    let detail = Resource::new(
        move || {
            (
                id.get(),
                a.sign.version().get(),
                a.unsign.version().get(),
                a.publish.version().get(),
                a.withdraw.version().get(),
                a.archive.version().get(),
            )
        },
        |(id, ..)| get_proposal(id),
    );

    view! {
        <Transition fallback=|| view! { <Loading/> }>
            {move || detail.get().map(|r| match r {
                Ok(d) => detail_view(d, a).into_any(),
                Err(error) => view! { <section class="page"><ErrorNotice error/></section> }.into_any(),
            })}
        </Transition>
        <div aria-live="polite">
            {move || matches!(a.sign.value().get(), Some(Ok(_))).then(|| view! { <p class="notice success">{t("proposals.signed")}</p> })}
            {move || matches!(a.unsign.value().get(), Some(Ok(_))).then(|| view! { <p class="notice success">{t("proposals.unsigned")}</p> })}
            {move || matches!(a.publish.value().get(), Some(Ok(_))).then(|| view! { <p class="notice success">{t("proposals.published")}</p> })}
            {move || matches!(a.withdraw.value().get(), Some(Ok(_))).then(|| view! { <p class="notice success">{t("proposals.withdrawn")}</p> })}
            {move || matches!(a.archive.value().get(), Some(Ok(_))).then(|| view! { <p class="notice success">{t("proposals.archived_ok")}</p> })}
            {move || action_error(a.sign.value().get())}
            {move || action_error(a.unsign.value().get())}
            {move || action_error(a.publish.value().get())}
            {move || action_error(a.withdraw.value().get())}
            {move || action_error(a.archive.value().get())}
        </div>
    }
}

fn detail_view(d: ProposalDetail, a: Actions) -> impl IntoView {
    let s = d.summary.clone();
    let id = s.id.clone();
    let status = s.status;
    let published = status != ProposalStatus::Draft;
    let viewer = d.viewer.clone();
    let notice = match (status, &viewer) {
        (ProposalStatus::Archived, _) => Some("proposals.archived_notice"),
        (s, _) if s.is_terminal() => Some("proposals.closed_phase"),
        (ProposalStatus::Draft, _) => Some("proposals.draft_notice"),
        (ProposalStatus::CollectingSignatures, None) => Some("proposals.login_to_act"),
        (ProposalStatus::CollectingSignatures, Some(v)) if !v.eligible => {
            Some("proposals.not_eligible")
        }
        _ => None,
    };

    view! {
        <article class="page" data-testid="proposal-detail">
            <h1 data-testid="proposal-title">{s.title.clone()}</h1>
            <p class="row">
                <StatusBadge status/>
                <span class="badge">{t(s.kind.label_key())}</span>
                <span class="badge">{t(s.level.label_key())}" · "{s.jurisdiction.clone()}</span>
            </p>
            {notice.map(|k| view! { <p class="notice" role="status">{t(k)}</p> })}
            {d.archive_reason.clone().map(|r| view! { <p>{t("proposals.field.archive_reason")}": "{r}</p> })}
            <p class="lead">{s.summary.clone()}</p>
            {s.signature_target.map(|target| view! { <SignatureProgress count=s.signature_count target/> })}
            {(!d.deadlines.is_empty()).then(|| view! {
                <ul class="deadlines">
                    {d.deadlines.iter().map(|dl| view! {
                        <li>
                            {t(match dl.phase {
                                ProposalStatus::CollectingSignatures => "proposals.deadline.collect",
                                ProposalStatus::InDebate => "proposals.deadline.debate",
                                _ => "proposals.deadline.vote",
                            })}": "{dl.at.clone()}
                            {dl.passed.then(|| format!(" ({})", t("proposals.deadline.passed")))}
                        </li>
                    }).collect_view()}
                </ul>
                <p class="hint">{t("proposals.timezone_note")}</p>
            })}
            {viewer.clone().map(|v| actions_view(id.clone(), v, a))}
            <h2>{t("proposals.field.body")}</h2>
            <div class="proposal-body" data-testid="proposal-body">{d.body.clone()}</div>
            {d.proposed_setup.map(|params| view! {
                <h2>{t("proposals.proposed_setup")}</h2>
                <SetupParamsTable params/>
            })}
            <dl>
                <dt>{t("proposals.field.author")}</dt><dd>{d.author.clone()}</dd>
                {d.published_at.clone().map(|at| view! { <dt>{t("proposals.field.published_at")}</dt><dd>{at}</dd> })}
                {d.setup_version.map(|v| view! { <dt>{t("proposals.field.setup_version")}</dt><dd>{v}</dd> })}
                {d.text_hash.clone().map(|h| view! { <dt>{t("proposals.field.text_hash")}</dt><dd><code>{h}</code></dd> })}
            </dl>
            {published.then(|| view! {
                <p class="row">
                    <A href=format!("/propostas/{id}/assinantes")>{t("proposals.action.signers")}</A>
                    <a href=format!("/propostas/{id}/exportar") rel="external" download>{t("proposals.action.export")}</a>
                    <A href=format!("/propostas/{id}/debate") attr:data-testid="debate-link">{t("debate.link")}</A>
                </p>
            })}
        </article>
    }
}

fn actions_view(
    id: String,
    v: crate::domain::proposals::view::ViewerState,
    a: Actions,
) -> impl IntoView {
    let reauth_id = id.clone();
    let start = move |action: &'static str| move |_| a.reauth_for.set(Some(action));
    let unsign_id = id.clone();
    view! {
        <div class="card actions">
            {v.has_signed.then(|| view! { <p>{t("proposals.signed")}</p> })}
            <div class="row">
                {v.can_sign.then(|| view! {
                    <button type="button" data-testid="sign" on:click=start(REAUTH_SIGN)>{t("proposals.action.sign")}</button>
                })}
                {v.can_withdraw_signature.then(|| view! {
                    <button type="button" class="secondary" data-testid="unsign"
                        on:click=move |_| { a.unsign.dispatch(UnsignProposal { id: unsign_id.clone(), idempotency_key: new_idempotency_key() }); }>
                        {t("proposals.action.unsign")}
                    </button>
                })}
                {v.can_edit.then(|| view! {
                    <A href=format!("/propostas/{id}/editar") attr:class="button secondary">{t("proposals.action.edit")}</A>
                })}
                {v.can_publish.then(|| view! {
                    <button type="button" data-testid="publish" on:click=start(REAUTH_PUBLISH)>{t("proposals.action.publish")}</button>
                })}
            </div>
            {v.can_publish.then(|| view! { <p class="hint">{t("proposals.publish_hint")}</p> })}
            {move || a.reauth_for.get().map(|action| {
                let rid = reauth_id.clone();
                let on_granted = Callback::new(move |_| {
                    a.reauth_for.set(None);
                    let key = new_idempotency_key();
                    if action == REAUTH_SIGN {
                        a.sign.dispatch(SignProposal { id: rid.clone(), idempotency_key: key });
                    } else {
                        a.publish.dispatch(PublishProposal { id: rid.clone(), idempotency_key: key });
                    }
                });
                view! {
                    <p class="hint">{t("proposals.reauth_hint")}</p>
                    <ReauthPanel action=action.to_owned() resource=reauth_id.clone() on_granted/>
                }
            })}
            {v.can_withdraw.then(|| reason_form(withdraw_form(a, id.clone())))}
            {v.can_archive.then(|| reason_form(archive_form(a, id.clone())))}
        </div>
    }
}

fn withdraw_form(a: Actions, id: String) -> AnyView {
    view! {
        <ActionForm action=a.withdraw>
            <input type="hidden" name="id" value=id/>
            <label>{t("proposals.field.reason")}<textarea name="reason" required minlength="5" maxlength="500"></textarea></label>
            <button type="submit" class="danger">{t("proposals.action.withdraw")}</button>
        </ActionForm>
    }
    .into_any()
}

fn archive_form(a: Actions, id: String) -> AnyView {
    view! {
        <ActionForm action=a.archive>
            <input type="hidden" name="id" value=id/>
            <label>{t("proposals.field.reason")}<textarea name="reason" required minlength="5" maxlength="500"></textarea></label>
            <button type="submit" class="danger">{t("proposals.action.archive")}</button>
        </ActionForm>
    }
    .into_any()
}

fn reason_form(form: AnyView) -> impl IntoView {
    view! { <details><summary>{t("proposals.field.reason")}</summary>{form}</details> }
}

/// `/propostas/:id/assinantes`: count only until public profiles exist.
#[component]
pub fn SignersPage() -> impl IntoView {
    let id = proposal_id();
    let data = Resource::new(move || id.get(), proposal_signers);
    view! {
        <section class="page">
            <h1>{t("proposals.signers.title")}</h1>
            <Transition fallback=|| view! { <Loading/> }>
                {move || data.get().map(|r| match r {
                    Ok(s) => view! {
                        <p>{t("proposals.signers.count")}": "<strong data-testid="signer-count">{s.count}</strong></p>
                        {(!s.visibility_available).then(|| view! { <p class="hint">{t("proposals.signers.private")}</p> })}
                        <ul>{s.public_names.into_iter().map(|n| view! { <li>{n}</li> }).collect_view()}</ul>
                    }.into_any(),
                    Err(error) => view! { <ErrorNotice error/> }.into_any(),
                })}
            </Transition>
            <p><A href=move || format!("/propostas/{}", id.get())>{t("proposals.back")}</A></p>
        </section>
    }
}
