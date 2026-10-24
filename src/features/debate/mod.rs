//! Debate screens: threaded comments, amendments and the moderation queue.

pub mod api;
mod thread;

pub use thread::DebatePage;

use leptos::prelude::*;
use leptos_router::{components::A, hooks::use_params_map};

use crate::{
    domain::debate::{
        AMENDMENT_BODY_MAX, AMENDMENT_TITLE_MAX, AmendmentStatus, ModerationDecision, REASON_MAX,
        view::{AmendmentView, AmendmentsView, ReportedComment},
    },
    features::{
        admin::AdminNav,
        proposals::StatusBadge,
        ui::{Empty, ErrorNotice, Loading, action_error},
    },
    i18n::t,
};
use api::{ChangeAmendment, ModerateComment, ProposeAmendment, get_amendments, moderation_queue};
use thread::decision_value;

/// `/propostas/:id/debate/emenda`: suggest an amendment and list existing ones.
#[component]
pub fn AmendmentsPage() -> impl IntoView {
    let params = use_params_map();
    let id = Memo::new(move |_| params.read().get("id").unwrap_or_default());
    let propose = ServerAction::<ProposeAmendment>::new();
    let change = ServerAction::<ChangeAmendment>::new();
    let title = RwSignal::new(String::new());
    let body = RwSignal::new(String::new());
    let data = Resource::new(
        move || (id.get(), propose.version().get(), change.version().get()),
        |(id, ..)| get_amendments(id),
    );
    Effect::new(move |_| {
        if matches!(propose.value().get(), Some(Ok(_))) {
            title.set(String::new());
            body.set(String::new());
        }
    });
    let submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        propose.dispatch(ProposeAmendment {
            id: id.get_untracked(),
            title: title.get_untracked(),
            body: body.get_untracked(),
        });
    };
    let form = move || {
        view! {
            <form class="card" on:submit=submit data-testid="amendment-form">
                <h2>{t("debate.amendment.new")}</h2>
                <label>{t("debate.amendment.field.title")}
                    <input type="text" required minlength="5" maxlength=AMENDMENT_TITLE_MAX.to_string() data-testid="amendment-title"
                        prop:value=move || title.get() on:input=move |ev| title.set(event_target_value(&ev))/>
                </label>
                <label>{t("debate.amendment.field.body")}
                    <textarea required minlength="20" maxlength=AMENDMENT_BODY_MAX.to_string() data-testid="amendment-body"
                        prop:value=move || body.get() on:input=move |ev| body.set(event_target_value(&ev))></textarea>
                </label>
                <button type="submit" data-testid="amendment-submit" disabled=move || propose.pending().get()>
                    {t("debate.amendment.submit")}
                </button>
            </form>
        }
    };

    view! {
        <section class="page" data-testid="amendments-page">
            <div aria-live="polite">
                {move || matches!(propose.value().get(), Some(Ok(_))).then(|| view! { <p class="notice success">{t("debate.amendment.sent")}</p> })}
                {move || matches!(change.value().get(), Some(Ok(_))).then(|| view! { <p class="notice success">{t("debate.amendment.changed")}</p> })}
                {move || action_error(propose.value().get())}
                {move || action_error(change.value().get())}
            </div>
            <Transition fallback=|| view! { <Loading/> }>
                {move || data.get().map(|r| match r {
                    Ok(v) => {
                        let can_propose = v.block.is_none();
                        let head = header(&v);
                        view! {
                            {head}
                            {can_propose.then(form)}
                            {amendment_list(v.items, change)}
                        }.into_any()
                    }
                    Err(error) => view! { <ErrorNotice error/> }.into_any(),
                })}
            </Transition>
        </section>
    }
}

fn header(v: &AmendmentsView) -> AnyView {
    let id = v.proposal_id.clone();
    let block = v.block;
    view! {
        <h1>{t("debate.amendment.title")}": "{v.title.clone()}</h1>
        <p class="row">
            <StatusBadge status=v.status/>
            <A href=format!("/propostas/{id}")>{t("debate.back")}</A>
            <A href=format!("/propostas/{id}/debate")>{t("debate.back_debate")}</A>
        </p>
        <p class="lead">{t("debate.amendment.lead")}</p>
        <p class="hint">{t("debate.amendment.hash")}": "<code>{v.text_hash.clone()}</code></p>
        {block.map(|b| view! { <p class="notice" data-testid="debate-block">{t(b.label_key())}</p> })}
    }
    .into_any()
}

fn amendment_list(items: Vec<AmendmentView>, change: ServerAction<ChangeAmendment>) -> AnyView {
    if items.is_empty() {
        return view! { <Empty message=t("debate.amendment.empty")/> }.into_any();
    }
    view! {
        <ul class="list" data-testid="amendment-list">
            {items.into_iter().map(|a| {
                let status_class = format!("badge amendment-{}", a.status.as_db());
                let (withdraw_id, address_id) = (a.id.clone(), a.id.clone());
                view! {
                    <li class="card" data-testid="amendment">
                        <h3 data-testid="amendment-item-title">{a.title.clone()}</h3>
                        <p class="row">
                            <span class=status_class data-testid="amendment-status">{t(a.status.label_key())}</span>
                            <span>{a.author.clone()}</span>
                            {a.own.then(|| view! { <span class="hint">{t("debate.you")}</span> })}
                            <span class="hint">{a.created_at.clone()}</span>
                        </p>
                        {(!a.current_text).then(|| view! { <p class="hint">{t("debate.amendment.other_text")}</p> })}
                        <p class="comment-body">{a.body.clone()}</p>
                        {a.status_note.clone().map(|n| view! { <p class="notice">{n}</p> })}
                        {a.can_withdraw.then(|| view! {
                            <ActionForm action=change>
                                <input type="hidden" name="amendment_id" value=withdraw_id/>
                                <input type="hidden" name="change" value="Withdraw"/>
                                <input type="hidden" name="note" value=""/>
                                <button type="submit" class="secondary" data-testid="amendment-withdraw">{t("debate.amendment.withdraw")}</button>
                            </ActionForm>
                        })}
                        {(a.can_address && a.status == AmendmentStatus::Open).then(|| view! {
                            <details>
                                <summary>{t("debate.amendment.address")}</summary>
                                <ActionForm action=change>
                                    <input type="hidden" name="amendment_id" value=address_id/>
                                    <input type="hidden" name="change" value="Address"/>
                                    <label>{t("debate.amendment.note")}
                                        <textarea name="note" required minlength="5" maxlength=REASON_MAX.to_string()></textarea>
                                    </label>
                                    <button type="submit" data-testid="amendment-address">{t("debate.amendment.address")}</button>
                                </ActionForm>
                            </details>
                        })}
                    </li>
                }
            }).collect_view()}
        </ul>
    }
    .into_any()
}

/// `/admin/moderacao`: open reports grouped by comment (moderators only).
#[component]
pub fn ModerationPage() -> impl IntoView {
    let moderate = ServerAction::<ModerateComment>::new();
    let queue = Resource::new(move || moderate.version().get(), |_| moderation_queue());
    view! {
        <section class="page">
            <h1>{t("debate.queue.title")}</h1>
            <AdminNav/>
            <p class="hint">{t("debate.queue.lead")}</p>
            <div aria-live="polite">
                {move || matches!(moderate.value().get(), Some(Ok(_))).then(|| view! { <p class="notice success">{t("debate.moderate.done")}</p> })}
                {move || action_error(moderate.value().get())}
            </div>
            <Transition fallback=|| view! { <Loading/> }>
                {move || queue.get().map(|r| match r {
                    Err(error) => view! { <p class="notice error" role="alert" data-testid="admin-denied">{error.user_message()}</p> }.into_any(),
                    Ok(items) if items.is_empty() => view! { <Empty message=t("debate.queue.empty")/> }.into_any(),
                    Ok(items) => view! {
                        <ul class="list" data-testid="moderation-queue">
                            {items.into_iter().map(|item| reported_item(item, moderate)).collect_view()}
                        </ul>
                    }.into_any(),
                })}
            </Transition>
        </section>
    }
}

fn reported_item(item: ReportedComment, moderate: ServerAction<ModerateComment>) -> impl IntoView {
    let reasons = item
        .reasons
        .iter()
        .map(|(r, n)| format!("{} ({n})", t(r.label_key())))
        .collect::<Vec<_>>()
        .join(" · ");
    view! {
        <li class="card" data-testid="reported-comment">
            <p class="row">
                <A href=format!("/propostas/{}/debate", item.proposal_id)>{t("debate.queue.open_thread")}": "{item.proposal_title.clone()}</A>
                {item.removed.then(|| view! { <span class="badge">{t("debate.removed_badge")}</span> })}
            </p>
            <p class="comment-body" data-testid="reported-text">{item.body.clone()}</p>
            <p><strong>{t("debate.report.reason")}": "</strong>{reasons}</p>
            {(!item.details.is_empty()).then(|| view! {
                <ul>{item.details.iter().map(|d| view! { <li class="comment-body">{d.clone()}</li> }).collect_view()}</ul>
            })}
            <p class="hint">{t("debate.queue.since")}" "{item.first_at.clone()}</p>
            <ActionForm action=moderate>
                <input type="hidden" name="comment_id" value=item.comment_id.clone()/>
                <label>{t("debate.moderate.reason")}
                    <textarea name="reason" required minlength="5" maxlength=REASON_MAX.to_string()></textarea>
                </label>
                <div class="row">
                    {(!item.removed).then(|| view! {
                        <button type="submit" name="decision" value=decision_value(ModerationDecision::Remove) class="danger">{t("debate.moderate.remove")}</button>
                    })}
                    <button type="submit" name="decision" value=decision_value(ModerationDecision::Dismiss) class="secondary">{t("debate.moderate.dismiss")}</button>
                </div>
            </ActionForm>
        </li>
    }
}
