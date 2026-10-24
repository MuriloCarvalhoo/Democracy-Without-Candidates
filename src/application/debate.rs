//! Debate use cases: threaded comments with edit history, reports, moderation
//! (hides from the public view only, with reason, history and audit), thread
//! locks and amendments. Amendments never change the published/supported text.

use std::collections::{BTreeMap, HashMap, HashSet};

use chrono::{DateTime, Duration, Utc};
use chrono_tz::Tz;
use serde_json::json;
use turso::{Connection, Value};

use super::{
    actor::Actor,
    audit::{self, Event, Visibility},
    identity::profile,
    notify::{Notice, notify},
    proposals::{self, Ctx, ProposalRow, local},
};
use crate::{
    domain::{
        debate::{
            AMENDMENT_BODY_MAX, AMENDMENT_DAILY_LIMIT, AMENDMENT_TITLE_MAX, AmendmentChange,
            AmendmentStatus, COMMENT_MAX, DETAILS_MAX, MAX_DEPTH, ModerationDecision, PostBlock,
            RATE_LIMIT, RATE_WINDOW_MINUTES, REASON_MAX, REASON_MIN, ReportReason,
            THREADS_PER_PAGE, amendments_open, comments_open,
            view::{
                AmendmentView, AmendmentsView, CommentView, DebateView, DebateViewer, LockInfo,
                ReportedComment, Revision,
            },
        },
        identity::Permission,
        proposals::ProposalStatus,
    },
    error::AppError,
    i18n::t,
    infrastructure::{
        clock::{parse_ts, ts},
        crypto::new_id,
        db::{query_all, query_opt},
        flags::{self, Flags},
    },
};

/// Notification events outside the configurable categories (always delivered).
pub const EVENT_ACTIVITY: &str = "debate.activity";
pub const EVENT_MODERATION: &str = "debate.moderation";

fn clean(
    field: &str,
    value: &str,
    min: usize,
    max: usize,
    multiline: bool,
) -> Result<String, AppError> {
    let value = value.trim();
    let len = value.chars().count();
    let bad_char = value
        .chars()
        .any(|c| c.is_control() && !(multiline && matches!(c, '\n' | '\r' | '\t')));
    if len < min || len > max || bad_char {
        return Err(AppError::validation(
            field,
            format!("{} ({min}–{max})", t("debate.error.length")),
        ));
    }
    Ok(value.replace("\r\n", "\n"))
}

fn clean_reason(reason: &str) -> Result<String, AppError> {
    clean("reason", reason, REASON_MIN, REASON_MAX, true)
}

/// Published proposal (never a draft or a withdrawn, never-published draft).
async fn published(conn: &Connection, id: &str) -> Result<ProposalRow, AppError> {
    proposals::load(conn, id)
        .await?
        .filter(|p| p.published_at.is_some())
        .ok_or(AppError::NotFound)
}

/// The phase accepts posts: status allows it and its deadline has not passed.
fn phase_open(p: &ProposalRow, now: DateTime<Utc>, amendment: bool) -> bool {
    let status_ok = if amendment {
        amendments_open(p.status)
    } else {
        comments_open(p.status)
    };
    let deadline = match p.status {
        ProposalStatus::CollectingSignatures => &p.collect_ends_at,
        ProposalStatus::InDebate => &p.debate_ends_at,
        ProposalStatus::InVoting => &p.vote_ends_at,
        _ => &None,
    };
    status_ok
        && deadline
            .as_deref()
            .and_then(parse_ts)
            .is_none_or(|end| now < end)
}

async fn lock_info(
    conn: &Connection,
    proposal_id: &str,
    tz: Tz,
) -> Result<Option<LockInfo>, AppError> {
    query_opt(
        conn,
        "SELECT reason, changed_at FROM debate_locks WHERE proposal_id = ?1 AND locked = 1",
        [proposal_id],
    )
    .await?
    .map(|r| -> Result<LockInfo, AppError> {
        Ok(LockInfo {
            reason: r.get(0)?,
            at: local(&r.get::<String>(1)?, tz),
        })
    })
    .transpose()
}

fn block_for(
    actor: &Actor,
    p: &ProposalRow,
    now: DateTime<Utc>,
    locked: bool,
    amendment: bool,
) -> Option<PostBlock> {
    if !phase_open(p, now, amendment) {
        Some(PostBlock::PhaseClosed)
    } else if locked {
        Some(PostBlock::Locked)
    } else if !actor.can(Permission::CommentCreate) || !actor.participates_in(&p.jurisdiction_id) {
        Some(PostBlock::NotEligible)
    } else {
        None
    }
}

fn block_error(block: PostBlock) -> AppError {
    match block {
        PostBlock::PhaseClosed => AppError::PhaseClosed,
        PostBlock::Locked => AppError::Conflict(t("debate.error.locked").into()),
        PostBlock::NotEligible | PostBlock::Visitor => AppError::Forbidden,
    }
}

async fn require_can_post(
    conn: &Connection,
    actor: &Actor,
    p: &ProposalRow,
    now: DateTime<Utc>,
    amendment: bool,
) -> Result<(), AppError> {
    let locked = query_opt(
        conn,
        "SELECT 1 FROM debate_locks WHERE proposal_id = ?1 AND locked = 1",
        [p.id.as_str()],
    )
    .await?
    .is_some();
    match block_for(actor, p, now, locked, amendment) {
        Some(block) => Err(block_error(block)),
        None => Ok(()),
    }
}

async fn count(conn: &Connection, sql: &str, params: Vec<Value>) -> Result<u64, AppError> {
    Ok(query_opt(conn, sql, params)
        .await?
        .ok_or(AppError::Internal)?
        .get::<u64>(0)?)
}

/// Persisted rate limit: comment writes (new comments + edits) in the window.
async fn check_rate(conn: &Connection, actor: &Actor, now: DateTime<Utc>) -> Result<(), AppError> {
    let since = ts(now - Duration::minutes(RATE_WINDOW_MINUTES));
    let writes = count(
        conn,
        "SELECT COUNT(*) FROM comment_revisions r JOIN comments c ON c.id = r.comment_id
         WHERE c.author_id = ?1 AND r.created_at > ?2",
        vec![actor.user_id.as_str().into(), since.into()],
    )
    .await?;
    if writes >= u64::from(RATE_LIMIT) {
        return Err(AppError::RateLimited);
    }
    Ok(())
}

/// Notifies the proposal/amendment author once per unread notice and link.
async fn notify_activity(
    conn: &Connection,
    now: DateTime<Utc>,
    user_id: &str,
    title: &str,
    body: &str,
    link: &str,
) -> Result<(), AppError> {
    let pending = query_opt(
        conn,
        "SELECT 1 FROM notifications WHERE user_id = ?1 AND event = ?2 AND link = ?3 AND read_at IS NULL",
        (user_id, EVENT_ACTIVITY, link),
    )
    .await?
    .is_some();
    if pending {
        return Ok(());
    }
    notify(
        conn,
        now,
        user_id,
        Notice {
            event: EVENT_ACTIVITY,
            title,
            body,
            link: Some(link),
        },
    )
    .await
}

// ---------------------------------------------------------------- comments

struct CommentRow {
    id: String,
    proposal_id: String,
    author_id: String,
    root_id: String,
    depth: u8,
    body: String,
    removed: bool,
}

async fn load_comment(conn: &Connection, id: &str) -> Result<CommentRow, AppError> {
    let r = query_opt(
        conn,
        "SELECT id, proposal_id, author_id, root_id, depth, body, status FROM comments WHERE id = ?1",
        [id],
    )
    .await?
    .ok_or(AppError::NotFound)?;
    Ok(CommentRow {
        id: r.get(0)?,
        proposal_id: r.get(1)?,
        author_id: r.get(2)?,
        root_id: r.get(3)?,
        depth: u8::try_from(r.get::<i64>(4)?).map_err(|_| AppError::Internal)?,
        body: r.get(5)?,
        removed: r.get::<String>(6)? == "removed",
    })
}

/// Adds a top-level comment (`parent_id = None`) or a reply. Returns its id.
pub async fn post_comment(
    ctx: &Ctx<'_>,
    actor: &Actor,
    proposal_id: &str,
    parent_id: Option<&str>,
    body: &str,
) -> Result<String, AppError> {
    ctx.flags.require(flags::DEBATE)?;
    actor.require(Permission::CommentCreate)?;
    let body = clean("body", body, 1, COMMENT_MAX, true)?;
    let tx = ctx.db.write().await?;
    let p = published(&tx, proposal_id).await?;
    require_can_post(&tx, actor, &p, ctx.now, false).await?;
    let id = new_id();
    let (root, depth) = match parent_id {
        None => (id.clone(), 0),
        Some(parent) => {
            let parent = load_comment(&tx, parent).await?;
            if parent.proposal_id != p.id {
                return Err(AppError::NotFound);
            }
            if parent.removed {
                return Err(AppError::Conflict(t("debate.error.removed").into()));
            }
            if parent.depth >= MAX_DEPTH {
                return Err(AppError::validation("parent_id", t("debate.error.depth")));
            }
            (parent.root_id, parent.depth + 1)
        }
    };
    check_rate(&tx, actor, ctx.now).await?;
    let now = ts(ctx.now);
    tx.execute(
        "INSERT INTO comments (id, proposal_id, author_id, parent_id, root_id, depth, body, status, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'visible', ?8)",
        (id.as_str(), p.id.as_str(), actor.user_id.as_str(), parent_id, root.as_str(), i64::from(depth), body.as_str(), now.as_str()),
    )
    .await?;
    tx.execute(
        "INSERT INTO comment_revisions (comment_id, rev, body, created_at) VALUES (?1, 1, ?2, ?3)",
        (id.as_str(), body.as_str(), now.as_str()),
    )
    .await?;
    if parent_id.is_none() && p.author_id != actor.user_id {
        let link = format!("/propostas/{}/debate", p.id);
        notify_activity(
            &tx,
            ctx.now,
            &p.author_id,
            t("debate.notify.activity"),
            &p.title,
            &link,
        )
        .await?;
    }
    tx.commit().await?;
    Ok(id)
}

/// Author edit: keeps every previous version in the append-only history.
pub async fn edit_comment(
    ctx: &Ctx<'_>,
    actor: &Actor,
    comment_id: &str,
    body: &str,
) -> Result<(), AppError> {
    ctx.flags.require(flags::DEBATE)?;
    actor.require(Permission::CommentCreate)?;
    let body = clean("body", body, 1, COMMENT_MAX, true)?;
    let tx = ctx.db.write().await?;
    let c = load_comment(&tx, comment_id).await?;
    if c.author_id != actor.user_id {
        return Err(AppError::Forbidden);
    }
    if c.removed {
        return Err(AppError::Conflict(t("debate.error.removed").into()));
    }
    let p = published(&tx, &c.proposal_id).await?;
    require_can_post(&tx, actor, &p, ctx.now, false).await?;
    if c.body == body {
        return Ok(());
    }
    check_rate(&tx, actor, ctx.now).await?;
    let rev = count(
        &tx,
        "SELECT COALESCE(MAX(rev), 0) + 1 FROM comment_revisions WHERE comment_id = ?1",
        vec![c.id.as_str().into()],
    )
    .await?;
    let now = ts(ctx.now);
    tx.execute(
        "INSERT INTO comment_revisions (comment_id, rev, body, created_at) VALUES (?1, ?2, ?3, ?4)",
        (c.id.as_str(), rev as i64, body.as_str(), now.as_str()),
    )
    .await?;
    tx.execute(
        "UPDATE comments SET body = ?2, edited_at = ?3 WHERE id = ?1 AND status = 'visible'",
        (c.id.as_str(), body.as_str(), now.as_str()),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

/// Edit history, visible to the comment author and moderators only.
pub async fn history(
    conn: &Connection,
    flags: &Flags,
    tz: Tz,
    actor: &Actor,
    comment_id: &str,
) -> Result<Vec<Revision>, AppError> {
    flags.require(flags::DEBATE)?;
    let c = load_comment(conn, comment_id).await?;
    if c.author_id != actor.user_id && !actor.can(Permission::ModerationAct) {
        return Err(AppError::Forbidden);
    }
    query_all(
        conn,
        "SELECT rev, body, created_at FROM comment_revisions WHERE comment_id = ?1 ORDER BY rev",
        [comment_id],
    )
    .await?
    .iter()
    .map(|r| {
        Ok(Revision {
            rev: r.get(0)?,
            body: r.get(1)?,
            at: local(&r.get::<String>(2)?, tz),
        })
    })
    .collect()
}

/// One report per account and comment; never on own or removed comments.
pub async fn report_comment(
    ctx: &Ctx<'_>,
    actor: &Actor,
    comment_id: &str,
    reason: ReportReason,
    details: &str,
) -> Result<(), AppError> {
    ctx.flags.require(flags::DEBATE)?;
    actor.require(Permission::CommentCreate)?;
    let details = clean("details", details, 0, DETAILS_MAX, true)?;
    if reason == ReportReason::Other && details.is_empty() {
        return Err(AppError::validation(
            "details",
            t("debate.error.details_required"),
        ));
    }
    let tx = ctx.db.write().await?;
    let c = load_comment(&tx, comment_id).await?;
    published(&tx, &c.proposal_id).await?;
    if c.author_id == actor.user_id {
        return Err(AppError::Forbidden);
    }
    if c.removed {
        return Err(AppError::Conflict(t("debate.error.removed").into()));
    }
    if query_opt(
        &tx,
        "SELECT 1 FROM comment_reports WHERE comment_id = ?1 AND reporter_id = ?2",
        (c.id.as_str(), actor.user_id.as_str()),
    )
    .await?
    .is_some()
    {
        return Err(AppError::Conflict(
            t("debate.error.already_reported").into(),
        ));
    }
    tx.execute(
        "INSERT INTO comment_reports (id, comment_id, reporter_id, reason, details, status, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, 'open', ?6)",
        (new_id(), c.id.as_str(), actor.user_id.as_str(), reason.as_db(), details, ts(ctx.now)),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn record_moderation(
    tx: &Connection,
    now: DateTime<Utc>,
    actor: &Actor,
    proposal_id: &str,
    target_kind: &str,
    target_id: &str,
    action: &str,
    reason: &str,
    public_kind: Option<&str>,
) -> Result<(), AppError> {
    tx.execute(
        "INSERT INTO moderation_actions (id, actor_id, proposal_id, target_kind, target_id, action, reason, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        (new_id(), actor.user_id.as_str(), proposal_id, target_kind, target_id, action, reason, ts(now)),
    )
    .await?;
    audit::append(
        tx,
        now,
        Event {
            kind: "debate.moderation",
            visibility: Visibility::Restricted,
            actor_ref: Some(&actor.user_id),
            subject: Some(target_id),
            payload: json!({ "action": action, "target_kind": target_kind, "proposal_id": proposal_id, "reason": reason }),
        },
    )
    .await?;
    if let Some(kind) = public_kind {
        // Public, minimal: no content, no moderator reference.
        audit::append(
            tx,
            now,
            Event {
                kind,
                visibility: Visibility::Public,
                actor_ref: None,
                subject: Some(target_id),
                payload: json!({ "proposal_id": proposal_id }),
            },
        )
        .await?;
    }
    Ok(())
}

/// Moderator decision on a comment, with mandatory reason, history and audit.
/// Removal hides the comment from the public view; the content is kept.
pub async fn moderate_comment(
    ctx: &Ctx<'_>,
    actor: &Actor,
    comment_id: &str,
    decision: ModerationDecision,
    reason: &str,
) -> Result<(), AppError> {
    ctx.flags.require(flags::DEBATE)?;
    actor.require(Permission::ModerationAct)?;
    let reason = clean_reason(reason)?;
    let tx = ctx.db.write().await?;
    let c = load_comment(&tx, comment_id).await?;
    let now = ts(ctx.now);
    let (public_kind, notice) = match decision {
        ModerationDecision::Remove => {
            if c.removed {
                return Err(AppError::Conflict(t("debate.error.removed").into()));
            }
            tx.execute(
                "UPDATE comments SET status = 'removed', removed_at = ?2 WHERE id = ?1",
                (c.id.as_str(), now.as_str()),
            )
            .await?;
            resolve_reports(&tx, &c.id, "actioned", actor, &now).await?;
            (
                Some("debate.comment_removed"),
                Some(t("debate.notify.removed")),
            )
        }
        ModerationDecision::Restore => {
            if !c.removed {
                return Err(AppError::Conflict(t("debate.error.not_removed").into()));
            }
            tx.execute(
                "UPDATE comments SET status = 'visible', removed_at = NULL WHERE id = ?1",
                [c.id.as_str()],
            )
            .await?;
            (
                Some("debate.comment_restored"),
                Some(t("debate.notify.restored")),
            )
        }
        ModerationDecision::Dismiss => {
            if resolve_reports(&tx, &c.id, "dismissed", actor, &now).await? == 0 {
                return Err(AppError::Conflict(t("debate.error.no_reports").into()));
            }
            (None, None)
        }
    };
    record_moderation(
        &tx,
        ctx.now,
        actor,
        &c.proposal_id,
        "comment",
        &c.id,
        decision.as_db(),
        &reason,
        public_kind,
    )
    .await?;
    if let Some(title) = notice {
        let link = format!("/propostas/{}/debate", c.proposal_id);
        notify(
            &tx,
            ctx.now,
            &c.author_id,
            Notice {
                event: EVENT_MODERATION,
                title,
                body: &reason,
                link: Some(&link),
            },
        )
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

async fn resolve_reports(
    tx: &Connection,
    comment_id: &str,
    status: &str,
    actor: &Actor,
    now: &str,
) -> Result<u64, AppError> {
    Ok(tx
        .execute(
            "UPDATE comment_reports SET status = ?2, resolved_at = ?3, resolved_by = ?4
             WHERE comment_id = ?1 AND status = 'open'",
            (comment_id, status, now, actor.user_id.as_str()),
        )
        .await?)
}

/// Locks (no new comments, replies, edits or amendments) or unlocks a debate.
/// Never changes phases, signatures or results.
pub async fn set_lock(
    ctx: &Ctx<'_>,
    actor: &Actor,
    proposal_id: &str,
    lock: bool,
    reason: &str,
) -> Result<(), AppError> {
    ctx.flags.require(flags::DEBATE)?;
    actor.require(Permission::ModerationAct)?;
    let reason = clean_reason(reason)?;
    let tx = ctx.db.write().await?;
    let p = published(&tx, proposal_id).await?;
    let locked = query_opt(
        &tx,
        "SELECT 1 FROM debate_locks WHERE proposal_id = ?1 AND locked = 1",
        [p.id.as_str()],
    )
    .await?
    .is_some();
    if locked == lock {
        return Err(AppError::Conflict(
            t(if lock {
                "debate.error.already_locked"
            } else {
                "debate.error.not_locked"
            })
            .into(),
        ));
    }
    tx.execute(
        "INSERT INTO debate_locks (proposal_id, locked, reason, changed_by, changed_at) VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(proposal_id) DO UPDATE SET locked = excluded.locked, reason = excluded.reason,
           changed_by = excluded.changed_by, changed_at = excluded.changed_at",
        (p.id.as_str(), lock as i64, reason.as_str(), actor.user_id.as_str(), ts(ctx.now)),
    )
    .await?;
    let (action, public_kind) = if lock {
        ("lock", "debate.thread_locked")
    } else {
        ("unlock", "debate.thread_unlocked")
    };
    record_moderation(
        &tx,
        ctx.now,
        actor,
        &p.id,
        "thread",
        &p.id,
        action,
        &reason,
        Some(public_kind),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

/// Display label: public profile name, otherwise a neutral label.
async fn author_label(
    conn: &Connection,
    cache: &mut HashMap<String, String>,
    user_id: &str,
) -> Result<String, AppError> {
    if let Some(label) = cache.get(user_id) {
        return Ok(label.clone());
    }
    let label = match profile::public_view(conn, user_id).await? {
        Some(p) => p.name,
        None => t("debate.author.private").to_owned(),
    };
    cache.insert(user_id.to_owned(), label.clone());
    Ok(label)
}

fn placeholders(from: usize, n: usize) -> String {
    (from..from + n)
        .map(|i| format!("?{i}"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Public debate page: paginated top-level threads with their replies. Removed
/// comments keep their place without content (moderators also see the content).
pub async fn thread(
    conn: &Connection,
    flags: &Flags,
    now: DateTime<Utc>,
    tz: Tz,
    viewer: Option<&Actor>,
    proposal_id: &str,
    page: u32,
) -> Result<DebateView, AppError> {
    flags.require(flags::DEBATE)?;
    let p = published(conn, proposal_id).await?;
    let lock = lock_info(conn, &p.id, tz).await?;
    let moderator = viewer.is_some_and(|a| a.can(Permission::ModerationAct));
    let block = viewer.map(|a| block_for(a, &p, now, lock.is_some(), false));
    let threads = count(
        conn,
        "SELECT COUNT(*) FROM comments WHERE proposal_id = ?1 AND parent_id IS NULL",
        vec![p.id.as_str().into()],
    )
    .await?;
    let pages = threads.div_ceil(u64::from(THREADS_PER_PAGE)).max(1) as u32;
    let page = page.clamp(1, pages);
    let roots: Vec<String> = query_all(
        conn,
        "SELECT id FROM comments WHERE proposal_id = ?1 AND parent_id IS NULL
         ORDER BY created_at, rowid LIMIT ?2 OFFSET ?3",
        (
            p.id.as_str(),
            i64::from(THREADS_PER_PAGE),
            i64::from((page - 1) * THREADS_PER_PAGE),
        ),
    )
    .await?
    .iter()
    .map(|r| r.get::<String>(0))
    .collect::<Result<_, _>>()?;

    let mut rows = Vec::new();
    if !roots.is_empty() {
        let mut params: Vec<Value> = vec![p.id.as_str().into()];
        params.extend(roots.iter().map(|id| Value::from(id.as_str())));
        rows = query_all(
            conn,
            &format!(
                "SELECT id, parent_id, depth, author_id, body, status, created_at, edited_at FROM comments
                 WHERE proposal_id = ?1 AND root_id IN ({}) ORDER BY created_at, rowid",
                placeholders(2, roots.len())
            ),
            params,
        )
        .await?;
    }
    let (reported, open_reports) = match viewer {
        Some(a) => {
            let reported: HashSet<String> = query_all(
                conn,
                "SELECT r.comment_id FROM comment_reports r JOIN comments c ON c.id = r.comment_id
                 WHERE c.proposal_id = ?1 AND r.reporter_id = ?2",
                (p.id.as_str(), a.user_id.as_str()),
            )
            .await?
            .iter()
            .map(|r| r.get::<String>(0))
            .collect::<Result<_, _>>()?;
            let mut open = HashMap::new();
            if moderator {
                for r in query_all(
                    conn,
                    "SELECT r.comment_id, COUNT(*) FROM comment_reports r JOIN comments c ON c.id = r.comment_id
                     WHERE c.proposal_id = ?1 AND r.status = 'open' GROUP BY r.comment_id",
                    [p.id.as_str()],
                )
                .await?
                {
                    open.insert(r.get::<String>(0)?, r.get::<u32>(1)?);
                }
            }
            (reported, open)
        }
        None => (HashSet::new(), HashMap::new()),
    };

    let mut labels = HashMap::new();
    let mut by_id: HashMap<String, CommentView> = HashMap::new();
    let mut children: BTreeMap<Option<String>, Vec<String>> = BTreeMap::new();
    for r in rows {
        let id: String = r.get(0)?;
        let parent_id: Option<String> = r.get(1)?;
        let depth = u8::try_from(r.get::<i64>(2)?).map_err(|_| AppError::Internal)?;
        let author_id: String = r.get(3)?;
        let removed = r.get::<String>(5)? == "removed";
        let own = viewer.is_some_and(|a| a.user_id == author_id);
        let can_post = block == Some(None);
        let is_reported = reported.contains(&id);
        let view = CommentView {
            parent_id: parent_id.clone(),
            depth,
            author: author_label(conn, &mut labels, &author_id).await?,
            own,
            body: (!removed || moderator)
                .then(|| r.get::<String>(4))
                .transpose()?,
            removed,
            edited_at: r.get::<Option<String>>(7)?.map(|at| local(&at, tz)),
            created_at: local(&r.get::<String>(6)?, tz),
            can_reply: can_post && !removed && depth < MAX_DEPTH,
            can_edit: can_post && own && !removed,
            can_report: viewer.is_some_and(|a| a.can(Permission::CommentCreate))
                && !own
                && !removed
                && !is_reported,
            reported: is_reported,
            open_reports: moderator.then(|| open_reports.get(&id).copied().unwrap_or(0)),
            id: id.clone(),
        };
        children.entry(parent_id).or_default().push(id.clone());
        by_id.insert(id, view);
    }
    // Depth-first display order: each thread followed by its replies.
    let mut comments = Vec::with_capacity(by_id.len());
    let mut stack: Vec<String> = roots.iter().rev().cloned().collect();
    while let Some(id) = stack.pop() {
        if let Some(kids) = children.get(&Some(id.clone())) {
            stack.extend(kids.iter().rev().cloned());
        }
        if let Some(view) = by_id.remove(&id) {
            comments.push(view);
        }
    }
    let amendments = count(
        conn,
        "SELECT COUNT(*) FROM amendments WHERE proposal_id = ?1",
        vec![p.id.as_str().into()],
    )
    .await?;
    Ok(DebateView {
        viewer: viewer.map(|a| DebateViewer {
            block: block.flatten(),
            moderator,
            proposal_author: a.user_id == p.author_id,
        }),
        proposal_id: p.id,
        title: p.title,
        status: p.status,
        lock,
        comments,
        page,
        pages,
        threads,
        amendments,
    })
}

/// Open reports grouped by comment, oldest first (moderators only).
pub async fn moderation_queue(
    conn: &Connection,
    flags: &Flags,
    tz: Tz,
    actor: &Actor,
) -> Result<Vec<ReportedComment>, AppError> {
    flags.require(flags::DEBATE)?;
    actor.require(Permission::ModerationAct)?;
    let rows = query_all(
        conn,
        "SELECT c.id, c.proposal_id, p.title, c.body, c.status, r.reason, r.details, r.created_at
         FROM comment_reports r JOIN comments c ON c.id = r.comment_id JOIN proposals p ON p.id = c.proposal_id
         WHERE r.status = 'open' ORDER BY r.created_at, r.rowid LIMIT 500",
        (),
    )
    .await?;
    let mut out: Vec<ReportedComment> = Vec::new();
    for r in rows {
        let id: String = r.get(0)?;
        let reason = ReportReason::from_db(&r.get::<String>(5)?).ok_or(AppError::Internal)?;
        let details: String = r.get(6)?;
        let idx = match out.iter().position(|c| c.comment_id == id) {
            Some(i) => i,
            None => {
                out.push(ReportedComment {
                    comment_id: id,
                    proposal_id: r.get(1)?,
                    proposal_title: r.get(2)?,
                    body: r.get(3)?,
                    removed: r.get::<String>(4)? == "removed",
                    reasons: Vec::new(),
                    details: Vec::new(),
                    first_at: local(&r.get::<String>(7)?, tz),
                });
                out.len() - 1
            }
        };
        let item = &mut out[idx];
        match item.reasons.iter_mut().find(|(k, _)| *k == reason) {
            Some((_, n)) => *n += 1,
            None => item.reasons.push((reason, 1)),
        }
        if !details.is_empty() {
            item.details.push(details);
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------- amendments

/// Suggested change bound to the published text hash. Never edits the proposal.
pub async fn propose_amendment(
    ctx: &Ctx<'_>,
    actor: &Actor,
    proposal_id: &str,
    title: &str,
    body: &str,
) -> Result<String, AppError> {
    ctx.flags.require(flags::DEBATE)?;
    actor.require(Permission::CommentCreate)?;
    let title = clean("title", title, 5, AMENDMENT_TITLE_MAX, false)?;
    let body = clean("body", body, 20, AMENDMENT_BODY_MAX, true)?;
    let tx = ctx.db.write().await?;
    let p = published(&tx, proposal_id).await?;
    require_can_post(&tx, actor, &p, ctx.now, true).await?;
    let recent = count(
        &tx,
        "SELECT COUNT(*) FROM amendments WHERE author_id = ?1 AND created_at > ?2",
        vec![
            actor.user_id.as_str().into(),
            ts(ctx.now - Duration::hours(24)).into(),
        ],
    )
    .await?;
    if recent >= u64::from(AMENDMENT_DAILY_LIMIT) {
        return Err(AppError::RateLimited);
    }
    let text_hash = p.text_hash.clone().ok_or(AppError::Internal)?;
    let id = new_id();
    let now = ts(ctx.now);
    tx.execute(
        "INSERT INTO amendments (id, proposal_id, author_id, title, body, text_hash, status, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'open', ?7, ?7)",
        (id.as_str(), p.id.as_str(), actor.user_id.as_str(), title, body, text_hash, now),
    )
    .await?;
    if p.author_id != actor.user_id {
        let link = format!("/propostas/{}/debate/emenda", p.id);
        notify_activity(
            &tx,
            ctx.now,
            &p.author_id,
            t("debate.notify.amendment"),
            &p.title,
            &link,
        )
        .await?;
    }
    tx.commit().await?;
    Ok(id)
}

/// Withdrawal by the amendment author, or "addressed" (with a note) by the
/// proposal author. Final: an amendment leaves `open` once.
pub async fn change_amendment(
    ctx: &Ctx<'_>,
    actor: &Actor,
    amendment_id: &str,
    change: AmendmentChange,
    note: &str,
) -> Result<(), AppError> {
    ctx.flags.require(flags::DEBATE)?;
    actor.require(Permission::CommentCreate)?;
    let tx = ctx.db.write().await?;
    let row = query_opt(
        &tx,
        "SELECT proposal_id, author_id, status, title FROM amendments WHERE id = ?1",
        [amendment_id],
    )
    .await?
    .ok_or(AppError::NotFound)?;
    let (proposal_id, author_id, status, title): (String, String, String, String) =
        (row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?);
    let p = published(&tx, &proposal_id).await?;
    if status != AmendmentStatus::Open.as_db() {
        return Err(AppError::Conflict(
            t("debate.error.amendment_closed").into(),
        ));
    }
    let now = ts(ctx.now);
    match change {
        AmendmentChange::Withdraw => {
            if author_id != actor.user_id {
                return Err(AppError::Forbidden);
            }
            tx.execute(
                "UPDATE amendments SET status = 'withdrawn', updated_at = ?2 WHERE id = ?1 AND status = 'open'",
                (amendment_id, now.as_str()),
            )
            .await?;
        }
        AmendmentChange::Address => {
            if p.author_id != actor.user_id {
                return Err(AppError::Forbidden);
            }
            let note = clean("note", note, REASON_MIN, REASON_MAX, true)?;
            tx.execute(
                "UPDATE amendments SET status = 'addressed', status_note = ?2, updated_at = ?3 WHERE id = ?1 AND status = 'open'",
                (amendment_id, note, now.as_str()),
            )
            .await?;
            if author_id != actor.user_id {
                let link = format!("/propostas/{}/debate/emenda", p.id);
                notify_activity(
                    &tx,
                    ctx.now,
                    &author_id,
                    t("debate.notify.addressed"),
                    &title,
                    &link,
                )
                .await?;
            }
        }
    }
    tx.commit().await?;
    Ok(())
}

/// Public list of amendments of a published proposal, newest first.
pub async fn amendments(
    conn: &Connection,
    flags: &Flags,
    now: DateTime<Utc>,
    tz: Tz,
    viewer: Option<&Actor>,
    proposal_id: &str,
) -> Result<AmendmentsView, AppError> {
    flags.require(flags::DEBATE)?;
    let p = published(conn, proposal_id).await?;
    let current = p.text_hash.clone().ok_or(AppError::Internal)?;
    let locked = lock_info(conn, &p.id, tz).await?.is_some();
    let is_author = viewer.is_some_and(|a| a.user_id == p.author_id);
    let mut labels = HashMap::new();
    let mut items = Vec::new();
    for r in query_all(
        conn,
        "SELECT id, author_id, title, body, text_hash, status, status_note, created_at FROM amendments
         WHERE proposal_id = ?1 ORDER BY created_at DESC, rowid DESC LIMIT 200",
        [p.id.as_str()],
    )
    .await?
    {
        let author_id: String = r.get(1)?;
        let status = AmendmentStatus::from_db(&r.get::<String>(5)?).ok_or(AppError::Internal)?;
        let text_hash: String = r.get(4)?;
        let own = viewer.is_some_and(|a| a.user_id == author_id);
        let open = status == AmendmentStatus::Open;
        items.push(AmendmentView {
            id: r.get(0)?,
            author: author_label(conn, &mut labels, &author_id).await?,
            own,
            title: r.get(2)?,
            body: r.get(3)?,
            current_text: text_hash == current,
            text_hash,
            status,
            status_note: r.get(6)?,
            created_at: local(&r.get::<String>(7)?, tz),
            can_withdraw: open && own,
            can_address: open && is_author,
        });
    }
    Ok(AmendmentsView {
        block: match viewer {
            Some(a) => block_for(a, &p, now, locked, true),
            None => Some(PostBlock::Visitor),
        },
        proposal_id: p.id,
        title: p.title,
        status: p.status,
        text_hash: current,
        items,
    })
}
