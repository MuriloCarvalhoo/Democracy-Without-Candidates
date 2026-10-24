//! `/propostas/:id/debate`: threaded comments, replies, edits, reports and
//! moderation controls. User text is rendered only as escaped text nodes.

use leptos::prelude::*;
use leptos_router::{
    components::A,
    hooks::{use_params_map, use_query_map},
};

use super::api::{
    CommentHistory, EditComment, ModerateComment, PostComment, ReportComment, SetDebateLock,
    get_debate,
};
use crate::{
    domain::debate::{
        COMMENT_MAX, DETAILS_MAX, ModerationDecision, PostBlock, REASON_MAX, ReportReason,
        view::{CommentView, DebateView},
    },
    features::{
        proposals::StatusBadge,
        ui::{Empty, ErrorNotice, Loading, action_error},
    },
    i18n::t,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Panel {
    Reply,
    Edit,
    Report,
    Moderate,
    History,
}

/// Actions and UI state shared by every comment of the page.
#[derive(Clone, Copy)]
struct Ctl {
    post: ServerAction<PostComment>,
    edit: ServerAction<EditComment>,
    report: ServerAction<ReportComment>,
    moderate: ServerAction<ModerateComment>,
    lock: ServerAction<SetDebateLock>,
    history: ServerAction<CommentHistory>,
    /// Comment whose inline panel is open.
    open: RwSignal<Option<(String, Panel)>>,
    /// Text of the open reply/edit panel.
    draft: RwSignal<String>,
    /// Text of the top-level composer.
    compose: RwSignal<String>,
}

#[component]
pub fn DebatePage() -> impl IntoView {
    let params = use_params_map();
    let query = use_query_map();
    let id = Memo::new(move |_| params.read().get("id").unwrap_or_default());
    let page = Memo::new(move |_| query.read().get("page").and_then(|p| p.parse::<u32>().ok()));
    let c = Ctl {
        post: ServerAction::new(),
        edit: ServerAction::new(),
        report: ServerAction::new(),
        moderate: ServerAction::new(),
        lock: ServerAction::new(),
        history: ServerAction::new(),
        open: RwSignal::new(None),
        draft: RwSignal::new(String::new()),
        compose: RwSignal::new(String::new()),
    };
    let data = Resource::new(
        move || {
            (
                id.get(),
                page.get(),
                c.post.version().get(),
                c.edit.version().get(),
                c.report.version().get(),
                c.moderate.version().get(),
                c.lock.version().get(),
            )
        },
        |(id, page, ..)| get_debate(id, page),
    );
    // Clear and close forms only after the server confirmed the action.
    Effect::new(move |_| {
        if matches!(c.post.value().get(), Some(Ok(_))) {
            c.compose.set(String::new());
            c.draft.set(String::new());
            c.open.set(None);
        }
    });
    Effect::new(move |_| {
        let done = [
            matches!(c.edit.value().get(), Some(Ok(_))),
            matches!(c.report.value().get(), Some(Ok(_))),
            matches!(c.moderate.value().get(), Some(Ok(_))),
        ];
        if done.contains(&true) {
            c.open.set(None);
        }
    });

    view! {
        <section class="page" data-testid="debate-page">
            <div aria-live="polite">
                {move || matches!(c.post.value().get(), Some(Ok(_))).then(|| view! { <p class="notice success">{t("debate.comment.posted")}</p> })}
                {move || matches!(c.edit.value().get(), Some(Ok(_))).then(|| view! { <p class="notice success">{t("debate.edit.saved")}</p> })}
                {move || matches!(c.report.value().get(), Some(Ok(_))).then(|| view! { <p class="notice success">{t("debate.report.sent")}</p> })}
                {move || matches!(c.moderate.value().get(), Some(Ok(_))).then(|| view! { <p class="notice success">{t("debate.moderate.done")}</p> })}
                {move || action_error(c.post.value().get())}
                {move || action_error(c.edit.value().get())}
                {move || action_error(c.report.value().get())}
                {move || action_error(c.moderate.value().get())}
                {move || action_error(c.lock.value().get())}
            </div>
            <Transition fallback=|| view! { <Loading/> }>
                {move || data.get().map(|r| match r {
                    Ok(d) => debate_view(d, c).into_any(),
                    Err(error) => view! { <ErrorNotice error/> }.into_any(),
                })}
            </Transition>
        </section>
    }
}

fn debate_view(d: DebateView, c: Ctl) -> impl IntoView {
    let id = d.proposal_id.clone();
    let moderator = d.viewer.as_ref().is_some_and(|v| v.moderator);
    let block = match &d.viewer {
        None => Some(PostBlock::Visitor),
        Some(v) => v.block,
    };
    let locked = d.lock.is_some();
    let (prev, next) = (d.page > 1, d.page < d.pages);
    let page_href = {
        let id = id.clone();
        move |p: u32| format!("/propostas/{id}/debate?page={p}")
    };
    let comments = d.comments;
    view! {
        <h1>{t("debate.title")}": "<span data-testid="debate-proposal-title">{d.title.clone()}</span></h1>
        <p class="row">
            <StatusBadge status=d.status/>
            <A href=format!("/propostas/{id}")>{t("debate.back")}</A>
            <A href=format!("/propostas/{id}/debate/emenda") attr:data-testid="amendments-link">
                {t("debate.amendments_link")}" ("{d.amendments}")"
            </A>
        </p>
        <p class="hint">{t("debate.rules")}</p>
        {d.lock.map(|l| view! {
            <p class="notice locked" role="status" data-testid="debate-locked">
                <strong>{t("debate.locked")}</strong>": "{l.reason}" ("{t("debate.locked_at")}" "{l.at}")"
            </p>
        })}
        {block.map(|b| view! {
            <p class="notice" data-testid="debate-block">
                {t(b.label_key())}
                {(b == PostBlock::Visitor).then(|| view! { " " <A href="/login">{t("nav.login")}</A> })}
            </p>
        })}
        {block.is_none().then(|| composer(id.clone(), c))}
        {moderator.then(|| lock_form(id.clone(), locked, c))}
        <p class="hint" data-testid="debate-threads">{d.threads}" "{t("debate.threads")}</p>
        {if comments.is_empty() {
            view! { <Empty message=t("debate.empty")/> }.into_any()
        } else {
            view! {
                <ol class="comments" data-testid="comment-list">
                    {comments.into_iter().map(|cm| comment_item(cm, id.clone(), moderator, c)).collect_view()}
                </ol>
            }.into_any()
        }}
        {(d.pages > 1).then(|| view! {
            <nav class="row" aria-label=t("debate.page")>
                {prev.then(|| view! { <A href=page_href(d.page - 1)>{t("debate.prev")}</A> })}
                <span>{format!("{} {} / {}", t("debate.page"), d.page, d.pages)}</span>
                {next.then(|| view! { <A href=page_href(d.page + 1)>{t("debate.next")}</A> })}
            </nav>
        })}
    }
}

fn composer(id: String, c: Ctl) -> impl IntoView {
    let submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        c.post.dispatch(PostComment {
            id: id.clone(),
            parent_id: None,
            body: c.compose.get_untracked(),
        });
    };
    view! {
        <form class="card" on:submit=submit data-testid="comment-form">
            <label>{t("debate.comment.label")}
                <textarea name="body" required maxlength=COMMENT_MAX.to_string() data-testid="comment-body"
                    prop:value=move || c.compose.get()
                    on:input=move |ev| c.compose.set(event_target_value(&ev))></textarea>
            </label>
            <button type="submit" data-testid="comment-submit" disabled=move || c.post.pending().get()>
                {t("debate.comment.submit")}
            </button>
        </form>
    }
}

fn lock_form(id: String, locked: bool, c: Ctl) -> impl IntoView {
    let label = if locked {
        t("debate.unlock.submit")
    } else {
        t("debate.lock.submit")
    };
    view! {
        <details class="card">
            <summary>{t("debate.lock.title")}</summary>
            <p class="hint">{t("debate.lock.hint")}</p>
            <ActionForm action=c.lock>
                <input type="hidden" name="id" value=id/>
                <input type="hidden" name="locked" value=(!locked).to_string()/>
                <label>{t("debate.lock.reason")}
                    <textarea name="reason" required minlength="5" maxlength=REASON_MAX.to_string() data-testid="lock-reason"></textarea>
                </label>
                <button type="submit" class="secondary" data-testid="lock-submit">{label}</button>
            </ActionForm>
        </details>
    }
}

/// Toggles one inline panel of a comment (one open panel per page).
fn toggler(
    c: Ctl,
    cid: String,
    panel: Panel,
    prefill: Option<String>,
) -> impl Fn(leptos::ev::MouseEvent) + 'static {
    move |_| {
        let same = c
            .open
            .get_untracked()
            .is_some_and(|(id, p)| id == cid && p == panel);
        if same {
            c.open.set(None);
            return;
        }
        c.draft.set(prefill.clone().unwrap_or_default());
        if panel == Panel::History {
            c.history.dispatch(CommentHistory {
                comment_id: cid.clone(),
            });
        }
        c.open.set(Some((cid.clone(), panel)));
    }
}

fn comment_item(cm: CommentView, pid: String, moderator: bool, c: Ctl) -> impl IntoView {
    let cid = cm.id.clone();
    let open = {
        let cid = cid.clone();
        Memo::new(move |_| {
            c.open.with(|o| {
                o.as_ref()
                    .filter(|(id, _)| *id == cid)
                    .map(|(_, panel)| *panel)
            })
        })
    };
    let expanded = move |panel: Panel| move || (open.get() == Some(panel)).to_string();
    let body = match (cm.removed, cm.body.clone()) {
        (false, Some(text)) => {
            view! { <p class="comment-body" data-testid="comment-text">{text}</p> }.into_any()
        }
        (true, Some(text)) => view! {
            <p class="comment-removed" data-testid="comment-removed">{t("debate.removed")}</p>
            <p class="comment-body moderated" data-testid="comment-text">{text}</p>
        }
        .into_any(),
        _ => view! { <p class="comment-removed" data-testid="comment-removed">{t("debate.removed")}</p> }
            .into_any(),
    };
    let show_history = (cm.own || moderator) && cm.edited_at.is_some();
    let (removed, reports) = (cm.removed, cm.open_reports.unwrap_or(0));
    let panel_view = {
        let cid = cid.clone();
        move || {
            let panel = open.get()?;
            Some(match panel {
                Panel::Reply | Panel::Edit => {
                    text_form(panel, pid.clone(), cid.clone(), c).into_any()
                }
                Panel::Report => report_form(cid.clone(), c).into_any(),
                Panel::Moderate => decision_form(cid.clone(), removed, reports, c).into_any(),
                Panel::History => history_view(c).into_any(),
            })
        }
    };

    view! {
        <li class=format!("comment depth-{}", cm.depth) data-testid="comment" data-comment-id=cid.clone()>
            <article>
                <header class="row">
                    <strong data-testid="comment-author">{cm.author.clone()}</strong>
                    {cm.own.then(|| view! { <span class="hint">{t("debate.you")}</span> })}
                    <span class="hint">{cm.created_at.clone()}</span>
                    {cm.edited_at.clone().map(|at| view! {
                        <span class="badge" title=at data-testid="comment-edited">{t("debate.edited")}</span>
                    })}
                    {cm.open_reports.filter(|n| *n > 0).map(|n| view! {
                        <span class="badge" data-testid="comment-reports">{n}" "{t("debate.open_reports")}</span>
                    })}
                </header>
                {body}
                {cm.reported.then(|| view! { <p class="hint">{t("debate.reported")}</p> })}
                <div class="row comment-actions">
                    {cm.can_reply.then(|| view! {
                        <button type="button" class="link" data-testid="comment-reply" aria-expanded=expanded(Panel::Reply)
                            on:click=toggler(c, cid.clone(), Panel::Reply, None)>{t("debate.reply")}</button>
                    })}
                    {cm.can_edit.then(|| view! {
                        <button type="button" class="link" data-testid="comment-edit" aria-expanded=expanded(Panel::Edit)
                            on:click=toggler(c, cid.clone(), Panel::Edit, cm.body.clone())>{t("debate.edit")}</button>
                    })}
                    {show_history.then(|| view! {
                        <button type="button" class="link" data-testid="comment-history" aria-expanded=expanded(Panel::History)
                            on:click=toggler(c, cid.clone(), Panel::History, None)>{t("debate.history")}</button>
                    })}
                    {cm.can_report.then(|| view! {
                        <button type="button" class="link" data-testid="comment-report" aria-expanded=expanded(Panel::Report)
                            on:click=toggler(c, cid.clone(), Panel::Report, None)>{t("debate.report")}</button>
                    })}
                    {moderator.then(|| view! {
                        <button type="button" class="link" data-testid="moderate-toggle" aria-expanded=expanded(Panel::Moderate)
                            on:click=toggler(c, cid.clone(), Panel::Moderate, None)>{t("debate.moderate")}</button>
                    })}
                </div>
                {panel_view}
            </article>
        </li>
    }
}

fn decision_form(cid: String, removed: bool, reports: u32, c: Ctl) -> impl IntoView {
    view! {
        <ActionForm action=c.moderate attr:class="inline-form">
            <input type="hidden" name="comment_id" value=cid/>
            <label>{t("debate.moderate.reason")}
                <textarea name="reason" required minlength="5" maxlength=REASON_MAX.to_string() data-testid="moderate-reason"></textarea>
            </label>
            <div class="row">
                {(!removed).then(|| view! {
                    <button type="submit" name="decision" value=decision_value(ModerationDecision::Remove) class="danger" data-testid="moderate-remove">{t("debate.moderate.remove")}</button>
                })}
                {removed.then(|| view! {
                    <button type="submit" name="decision" value=decision_value(ModerationDecision::Restore) class="secondary" data-testid="moderate-restore">{t("debate.moderate.restore")}</button>
                })}
                {(reports > 0).then(|| view! {
                    <button type="submit" name="decision" value=decision_value(ModerationDecision::Dismiss) class="secondary" data-testid="moderate-dismiss">{t("debate.moderate.dismiss")}</button>
                })}
            </div>
        </ActionForm>
    }
}

/// Reply (new child comment) or edit of the own comment.
fn text_form(panel: Panel, pid: String, cid: String, c: Ctl) -> impl IntoView {
    let reply = panel == Panel::Reply;
    let submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let body = c.draft.get_untracked();
        if reply {
            c.post.dispatch(PostComment {
                id: pid.clone(),
                parent_id: Some(cid.clone()),
                body,
            });
        } else {
            c.edit.dispatch(EditComment {
                comment_id: cid.clone(),
                body,
            });
        }
    };
    let (label, action, testid) = if reply {
        (
            t("debate.reply.label"),
            t("debate.reply.submit"),
            "reply-submit",
        )
    } else {
        (
            t("debate.edit.label"),
            t("debate.edit.submit"),
            "edit-submit",
        )
    };
    view! {
        <form on:submit=submit class="inline-form">
            <label>{label}
                <textarea required maxlength=COMMENT_MAX.to_string() data-testid=if reply { "reply-body" } else { "edit-body" }
                    prop:value=move || c.draft.get()
                    on:input=move |ev| c.draft.set(event_target_value(&ev))></textarea>
            </label>
            <div class="row">
                <button type="submit" data-testid=testid
                    disabled=move || c.post.pending().get() || c.edit.pending().get()>{action}</button>
                <button type="button" class="secondary" on:click=move |_| c.open.set(None)>{t("debate.cancel")}</button>
            </div>
        </form>
    }
}

fn report_form(cid: String, c: Ctl) -> impl IntoView {
    view! {
        <ActionForm action=c.report attr:class="inline-form">
            <input type="hidden" name="comment_id" value=cid/>
            <label>{t("debate.report.reason")}
                <select name="reason" data-testid="report-reason">
                    {ReportReason::ALL.into_iter().map(|r| view! {
                        <option value=format!("{r:?}")>{t(r.label_key())}</option>
                    }).collect_view()}
                </select>
            </label>
            <label>{t("debate.report.details")}
                <textarea name="details" maxlength=DETAILS_MAX.to_string() data-testid="report-details"></textarea>
            </label>
            <button type="submit" data-testid="report-submit">{t("debate.report.submit")}</button>
        </ActionForm>
    }
}

fn history_view(c: Ctl) -> impl IntoView {
    move || {
        c.history.value().get().map(|r| match r {
            Ok(revisions) => view! {
                <ol class="history" data-testid="comment-revisions">
                    {revisions.into_iter().map(|rev| view! {
                        <li>
                            <span class="hint">{t("debate.history.rev")}" "{rev.rev}" · "{rev.at}</span>
                            <p class="comment-body">{rev.body}</p>
                        </li>
                    }).collect_view()}
                </ol>
            }
            .into_any(),
            Err(error) => view! { <ErrorNotice error/> }.into_any(),
        })
    }
}

/// Form value of a decision (serde variant name).
pub(super) fn decision_value(d: ModerationDecision) -> &'static str {
    match d {
        ModerationDecision::Remove => "Remove",
        ModerationDecision::Restore => "Restore",
        ModerationDecision::Dismiss => "Dismiss",
    }
}
