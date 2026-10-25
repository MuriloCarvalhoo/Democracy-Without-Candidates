//! Proposal use cases: drafts, publication (setup, quota, reauth), withdrawal,
//! administrative archiving and public read models.

use chrono::{DateTime, Datelike, Utc};
use chrono_tz::Tz;
use serde_json::json;
use turso::{Connection, Row, Value};

use super::{
    actor::Actor,
    audit::{self, Event, Visibility},
    idempotency::{self, Key},
    notify::{Notice, notify},
    phases, reauth, setup,
};
use crate::{
    domain::{
        identity::Permission,
        proposals::{
            BODY_MAX, Level, ProposalKind, ProposalStatus, REASON_MAX, SUMMARY_MAX, TITLE_MAX,
            can_transition, min_count, normalize_text,
            view::{
                Dashboard, Deadline, Draft, DraftInput, DraftSaved, DuplicateHit, ProposalDetail,
                ProposalPage, ProposalSummary, Published, Signers, ViewerState,
            },
        },
    },
    error::AppError,
    i18n::t,
    infrastructure::{
        clock::{parse_ts, ts},
        crypto::{new_id, sha256_hex},
        db::{Db, query_all, query_opt},
        flags::{self, Flags},
    },
};

pub const PAGE_SIZE: u32 = 20;
pub use crate::domain::proposals::REAUTH_PUBLISH;

/// Request context for proposal use cases.
pub struct Ctx<'a> {
    pub db: &'a Db,
    pub flags: &'a Flags,
    pub now: DateTime<Utc>,
    /// Instance timezone: civil year for quotas and displayed instants.
    pub tz: Tz,
}

/// Persisted proposal columns used by the use cases.
#[derive(Debug, Clone)]
pub struct ProposalRow {
    pub id: String,
    pub kind: ProposalKind,
    pub status: ProposalStatus,
    pub jurisdiction_id: String,
    pub author_id: String,
    pub title: String,
    pub summary: String,
    pub body: String,
    pub text_hash: Option<String>,
    pub proposed_setup_id: Option<String>,
    pub setup_version_id: Option<String>,
    pub collection_snapshot_id: Option<String>,
    pub signature_target: Option<u64>,
    pub signature_count: u64,
    pub collect_ends_at: Option<String>,
    pub debate_ends_at: Option<String>,
    pub vote_ends_at: Option<String>,
    pub vote_setup_version_id: Option<String>,
    pub vote_snapshot_id: Option<String>,
    pub published_at: Option<String>,
    pub archive_reason: Option<String>,
}

const ROW_COLUMNS: &str = "id, kind, status, jurisdiction_id, author_id, title, summary, body, text_hash, proposed_setup_id, setup_version_id, collection_snapshot_id, signature_target, signature_count, collect_ends_at, debate_ends_at, vote_ends_at, vote_setup_version_id, vote_snapshot_id, published_at, archive_reason";

fn row_from(row: &Row) -> Result<ProposalRow, AppError> {
    Ok(ProposalRow {
        id: row.get(0)?,
        kind: ProposalKind::from_db(&row.get::<String>(1)?).ok_or(AppError::Internal)?,
        status: ProposalStatus::from_db(&row.get::<String>(2)?).ok_or(AppError::Internal)?,
        jurisdiction_id: row.get(3)?,
        author_id: row.get(4)?,
        title: row.get(5)?,
        summary: row.get(6)?,
        body: row.get(7)?,
        text_hash: row.get(8)?,
        proposed_setup_id: row.get(9)?,
        setup_version_id: row.get(10)?,
        collection_snapshot_id: row.get(11)?,
        signature_target: row.get(12)?,
        signature_count: row.get(13)?,
        collect_ends_at: row.get(14)?,
        debate_ends_at: row.get(15)?,
        vote_ends_at: row.get(16)?,
        vote_setup_version_id: row.get(17)?,
        vote_snapshot_id: row.get(18)?,
        published_at: row.get(19)?,
        archive_reason: row.get(20)?,
    })
}

pub async fn load(conn: &Connection, id: &str) -> Result<Option<ProposalRow>, AppError> {
    match query_opt(
        conn,
        &format!("SELECT {ROW_COLUMNS} FROM proposals WHERE id = ?1"),
        [id],
    )
    .await?
    {
        Some(row) => Ok(Some(row_from(&row)?)),
        None => Ok(None),
    }
}

pub async fn level_of(conn: &Connection, jurisdiction_id: &str) -> Result<Level, AppError> {
    let kind: String = query_opt(
        conn,
        "SELECT kind FROM jurisdictions WHERE id = ?1",
        [jurisdiction_id],
    )
    .await?
    .ok_or(AppError::NotFound)?
    .get(0)?;
    Level::from_db(&kind).ok_or(AppError::Internal)
}

/// "Blumenau/SC", "Santa Catarina", "Brasil".
pub async fn jurisdiction_label(conn: &Connection, id: &str) -> Result<String, AppError> {
    let row = query_opt(
        conn,
        "SELECT kind, name, state_abbrev FROM jurisdictions WHERE id = ?1",
        [id],
    )
    .await?
    .ok_or(AppError::NotFound)?;
    Ok(label(&row.get::<String>(0)?, row.get(1)?, row.get(2)?))
}

fn label(kind: &str, name: String, abbrev: Option<String>) -> String {
    match (kind, abbrev) {
        ("municipal", Some(uf)) => format!("{name}/{uf}"),
        _ => name,
    }
}

/// Instant formatted in the instance timezone for display.
pub fn local(at: &str, tz: Tz) -> String {
    parse_ts(at)
        .map(|at| at.with_timezone(&tz).format("%d/%m/%Y %H:%M").to_string())
        .unwrap_or_default()
}

/// SHA-256 of the published text (`title`, blank line, `summary`, blank line, `body`).
pub fn text_hash(title: &str, summary: &str, body: &str) -> String {
    sha256_hex(format!("{title}\n\n{summary}\n\n{body}"))
}

fn author_jurisdiction(actor: &Actor, level: Level) -> &str {
    match level {
        Level::National => &actor.country_id,
        Level::State => &actor.state_id,
        Level::Municipal => &actor.municipality_id,
    }
}

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
            format!("{} ({min}–{max})", t("proposals.error.length")),
        ));
    }
    Ok(value.replace("\r\n", "\n"))
}

pub fn clean_reason(reason: &str) -> Result<String, AppError> {
    clean("reason", reason, 5, REASON_MAX, true)
}

async fn duplicates(
    conn: &Connection,
    id: &str,
    normalized_title: &str,
    body_hash: &str,
) -> Result<Vec<DuplicateHit>, AppError> {
    let rows = query_all(
        conn,
        "SELECT id, title, status FROM proposals
         WHERE id <> ?1 AND status <> 'draft' AND (normalized_title = ?2 OR normalized_body_hash = ?3)
         ORDER BY published_at DESC LIMIT 5",
        (id, normalized_title, body_hash),
    )
    .await?;
    rows.iter()
        .map(|r| {
            Ok(DuplicateHit {
                id: r.get(0)?,
                title: r.get(1)?,
                status: ProposalStatus::from_db(&r.get::<String>(2)?).ok_or(AppError::Internal)?,
            })
        })
        .collect()
}

/// Creates (`id = None`) or updates the author's draft. Laws require an adopted
/// setup for the jurisdiction; `setup_change` carries a proposed parameter package.
/// Returns duplicate warnings (never blocking).
pub async fn save_draft(
    ctx: &Ctx<'_>,
    actor: &Actor,
    id: Option<&str>,
    input: &DraftInput,
) -> Result<DraftSaved, AppError> {
    ctx.flags.require(flags::PROPOSALS)?;
    actor.require(Permission::ProposalCreate)?;
    if !input.kind.citizen_creatable() {
        return Err(AppError::Forbidden);
    }
    let title = clean("title", &input.title, 5, TITLE_MAX, false)?;
    let summary = clean("summary", &input.summary, 10, SUMMARY_MAX, true)?;
    let body = clean("body", &input.body, 20, BODY_MAX, true)?;
    let proposed = match (input.kind, input.proposed_setup) {
        (ProposalKind::SetupChange, Some(params)) => {
            params
                .validate()
                .map_err(|f| AppError::validation(f, t("proposals.error.setup_param")))?;
            Some(params)
        }
        (ProposalKind::SetupChange, None) => {
            return Err(AppError::validation(
                "proposed_setup",
                t("proposals.error.setup_param"),
            ));
        }
        _ => None,
    };
    let jurisdiction = author_jurisdiction(actor, input.level).to_owned();
    let normalized_title = normalize_text(&title);
    let body_hash = sha256_hex(normalize_text(&body));

    let tx = ctx.db.write().await?;
    if setup::effective_setup(&tx, &jurisdiction).await?.is_none() {
        return Err(AppError::Conflict(t("proposals.error.no_setup").into()));
    }
    let id = match id {
        Some(id) => {
            let p = load(&tx, id).await?.ok_or(AppError::NotFound)?;
            if p.author_id != actor.user_id {
                return Err(AppError::NotFound);
            }
            if p.status != ProposalStatus::Draft {
                return Err(AppError::PhaseClosed);
            }
            if let Some(old) = &p.proposed_setup_id {
                tx.execute(
                    "UPDATE setup_versions SET status = 'rejected' WHERE id = ?1 AND status = 'proposed'",
                    [old.as_str()],
                )
                .await?;
            }
            p.id
        }
        None => {
            let id = new_id();
            tx.execute(
                "INSERT INTO proposals (id, kind, jurisdiction_id, author_id, title, summary, body, normalized_title, normalized_body_hash, status, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, '', '', '', '', '', 'draft', ?5, ?5)",
                (id.as_str(), input.kind.as_db(), jurisdiction.as_str(), actor.user_id.as_str(), ts(ctx.now)),
            )
            .await?;
            id
        }
    };
    let proposed_id = match proposed {
        Some(params) => {
            Some(setup::insert_proposed(&tx, ctx.now, &jurisdiction, &params, false).await?)
        }
        None => None,
    };
    tx.execute(
        "UPDATE proposals SET kind = ?2, jurisdiction_id = ?3, title = ?4, summary = ?5, body = ?6,
           normalized_title = ?7, normalized_body_hash = ?8, proposed_setup_id = ?9, updated_at = ?10
         WHERE id = ?1 AND status = 'draft'",
        (
            id.as_str(),
            input.kind.as_db(),
            jurisdiction.as_str(),
            title,
            summary,
            body,
            normalized_title.as_str(),
            body_hash.as_str(),
            proposed_id,
            ts(ctx.now),
        ),
    )
    .await?;
    let duplicates = duplicates(&tx, &id, &normalized_title, &body_hash).await?;
    tx.commit().await?;
    Ok(DraftSaved { id, duplicates })
}

/// Draft fields for the author's edit form.
pub async fn get_draft(conn: &Connection, actor: &Actor, id: &str) -> Result<Draft, AppError> {
    let p = load(conn, id).await?.ok_or(AppError::NotFound)?;
    if p.author_id != actor.user_id {
        return Err(AppError::NotFound);
    }
    if p.status != ProposalStatus::Draft {
        return Err(AppError::PhaseClosed);
    }
    let proposed_setup = match &p.proposed_setup_id {
        Some(sid) => setup::get(conn, sid).await?.map(|s| s.params),
        None => None,
    };
    Ok(Draft {
        level: level_of(conn, &p.jurisdiction_id).await?,
        id: p.id,
        kind: p.kind,
        title: p.title,
        summary: p.summary,
        body: p.body,
        proposed_setup,
    })
}

/// Publishes a draft: opens signature collection with the effective setup, an
/// electorate snapshot and the signature target. Consumes the `proposal:create`
/// reauth grant and the yearly quota in the same transaction.
pub async fn publish(
    ctx: &Ctx<'_>,
    actor: &Actor,
    id: &str,
    idempotency_key: Option<&str>,
) -> Result<Published, AppError> {
    ctx.flags.require(flags::PROPOSALS)?;
    actor.require(Permission::ProposalCreate)?;
    let key = idempotency::parse(idempotency_key)?.map(|key| Key {
        user_id: &actor.user_id,
        key,
        action: "proposal.publish",
        request_hash: idempotency::request_hash(&[id]),
    });
    let tx = ctx.db.write().await?;
    if let Some(key) = &key
        && let Some(previous) = idempotency::lookup(&tx, key).await?
    {
        return Ok(previous);
    }
    let p = load(&tx, id).await?.ok_or(AppError::NotFound)?;
    if p.author_id != actor.user_id {
        return Err(AppError::NotFound);
    }
    if !can_transition(p.kind, p.status, ProposalStatus::CollectingSignatures) {
        return Err(AppError::PhaseClosed);
    }
    if !actor.participates_in(&p.jurisdiction_id) {
        return Err(AppError::Forbidden);
    }
    let setup = setup::effective_setup(&tx, &p.jurisdiction_id)
        .await?
        .ok_or_else(|| AppError::Conflict(t("proposals.error.no_setup").into()))?;
    let rules = setup.params.rules(p.kind);
    let year = ctx.now.with_timezone(&ctx.tz).year();
    if let Some(limit) = rules.quota_per_year {
        let used: u32 = query_opt(
            &tx,
            "SELECT COUNT(*) FROM proposals WHERE author_id = ?1 AND kind = ?2 AND quota_year = ?3",
            (actor.user_id.as_str(), p.kind.as_db(), year),
        )
        .await?
        .ok_or(AppError::Internal)?
        .get(0)?;
        if used >= limit {
            return Err(AppError::Conflict(t("proposals.error.quota").into()));
        }
    }
    reauth::consume(&tx, ctx.now, &actor.session_hash, REAUTH_PUBLISH, id).await?;
    let (snapshot_id, electorate) = phases::snapshot(
        &tx,
        ctx.now,
        id,
        phases::Phase::Collection,
        &p.jurisdiction_id,
        &setup.id,
    )
    .await?;
    let level = level_of(&tx, &p.jurisdiction_id).await?;
    let target = min_count(setup.params.signature_bp(level), electorate).max(1);
    let collect_days = rules.collect_days.ok_or(AppError::Internal)?;
    let collect_ends_at = ts(ctx.now + chrono::Duration::days(collect_days.into()));
    let hash = text_hash(&p.title, &p.summary, &p.body);
    let quota_year = rules.quota_per_year.map(|_| year);
    let changed = tx
        .execute(
            "UPDATE proposals SET status = 'collecting_signatures', text_hash = ?2, setup_version_id = ?3,
               collection_snapshot_id = ?4, signature_target = ?5, quota_year = ?6, collect_ends_at = ?7,
               published_at = ?8, updated_at = ?8
             WHERE id = ?1 AND status = 'draft'",
            (
                id,
                hash.as_str(),
                setup.id.as_str(),
                snapshot_id.as_str(),
                target,
                quota_year,
                collect_ends_at.as_str(),
                ts(ctx.now),
            ),
        )
        .await?;
    if changed != 1 {
        return Err(AppError::PhaseClosed);
    }
    audit::append(
        &tx,
        ctx.now,
        Event {
            kind: "proposal.published",
            visibility: Visibility::Public,
            actor_ref: Some(&actor.user_id),
            subject: Some(id),
            payload: json!({
                "kind": p.kind.as_db(),
                "jurisdiction_id": p.jurisdiction_id,
                "text_hash": hash,
                "setup_version_id": setup.id,
                "electorate": electorate,
                "signature_target": target,
                "collect_ends_at": collect_ends_at,
            }),
        },
    )
    .await?;
    let link = format!("/propostas/{id}");
    notify(
        &tx,
        ctx.now,
        &actor.user_id,
        Notice {
            event: "proposal.published",
            title: t("proposals.notify.published"),
            body: &p.title,
            link: Some(&link),
        },
    )
    .await?;
    let result = Published {
        id: id.to_owned(),
        status: ProposalStatus::CollectingSignatures,
        signature_target: target,
        collect_ends_at,
    };
    if let Some(key) = &key {
        idempotency::store(&tx, ctx.now, key, &result).await?;
    }
    tx.commit().await?;
    Ok(result)
}

/// Author withdrawal of a draft or a proposal in collection; ends in `archived`.
/// The consumed quota is not refunded.
pub async fn withdraw(
    ctx: &Ctx<'_>,
    actor: &Actor,
    id: &str,
    reason: &str,
) -> Result<(), AppError> {
    let reason = clean_reason(reason)?;
    let tx = ctx.db.write().await?;
    let p = load(&tx, id).await?.ok_or(AppError::NotFound)?;
    if p.author_id != actor.user_id {
        return Err(if p.status == ProposalStatus::Draft {
            AppError::NotFound
        } else {
            AppError::Forbidden
        });
    }
    if !matches!(
        p.status,
        ProposalStatus::Draft | ProposalStatus::CollectingSignatures
    ) || !can_transition(p.kind, p.status, ProposalStatus::Archived)
    {
        return Err(AppError::PhaseClosed);
    }
    archive_row(
        &tx,
        ctx.now,
        &p,
        &reason,
        "proposal.withdrawn",
        Some(&actor.user_id),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

/// Administrative archiving, only from terminal phases; history is preserved.
pub async fn archive(ctx: &Ctx<'_>, actor: &Actor, id: &str, reason: &str) -> Result<(), AppError> {
    actor.require(Permission::ProposalArchive)?;
    let reason = clean_reason(reason)?;
    let tx = ctx.db.write().await?;
    let p = load(&tx, id).await?.ok_or(AppError::NotFound)?;
    if !p.status.is_terminal() {
        return Err(AppError::PhaseClosed);
    }
    archive_row(&tx, ctx.now, &p, &reason, "proposal.archived", None).await?;
    let link = format!("/propostas/{id}");
    notify(
        &tx,
        ctx.now,
        &p.author_id,
        Notice {
            event: "proposal.archived",
            title: t("proposals.notify.archived"),
            body: &p.title,
            link: Some(&link),
        },
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

async fn archive_row(
    tx: &Connection,
    now: DateTime<Utc>,
    p: &ProposalRow,
    reason: &str,
    event: &str,
    actor_ref: Option<&str>,
) -> Result<(), AppError> {
    let changed = tx
        .execute(
            "UPDATE proposals SET status = 'archived', archived_at = ?3, archive_reason = ?4, updated_at = ?3,
               closed_at = COALESCE(closed_at, ?3)
             WHERE id = ?1 AND status = ?2",
            (p.id.as_str(), p.status.as_db(), ts(now), reason),
        )
        .await?;
    if changed != 1 {
        return Err(AppError::PhaseClosed);
    }
    if let Some(sid) = &p.proposed_setup_id {
        tx.execute(
            "UPDATE setup_versions SET status = 'rejected' WHERE id = ?1 AND status = 'proposed'",
            [sid.as_str()],
        )
        .await?;
    }
    if p.status != ProposalStatus::Draft {
        audit::append(
            tx,
            now,
            Event {
                kind: event,
                visibility: Visibility::Public,
                actor_ref,
                subject: Some(&p.id),
                payload: json!({ "from": p.status.as_db(), "reason": reason }),
            },
        )
        .await?;
    }
    Ok(())
}

// ---------------------------------------------------------------- read models

const SUMMARY_SQL: &str =
    "SELECT p.id, p.kind, p.status, j.kind, j.name, j.state_abbrev, p.title, p.summary,
    p.signature_count, p.signature_target, p.collect_ends_at, p.debate_ends_at, p.vote_ends_at
    FROM proposals p JOIN jurisdictions j ON j.id = p.jurisdiction_id";

fn summary_from(row: &Row, tz: Tz) -> Result<ProposalSummary, AppError> {
    let status = ProposalStatus::from_db(&row.get::<String>(2)?).ok_or(AppError::Internal)?;
    let jkind: String = row.get(3)?;
    let deadline_col = match status {
        ProposalStatus::CollectingSignatures => Some(10),
        ProposalStatus::InDebate => Some(11),
        ProposalStatus::InVoting => Some(12),
        _ => None,
    };
    let deadline = match deadline_col {
        Some(i) => row.get::<Option<String>>(i)?.map(|at| local(&at, tz)),
        None => None,
    };
    Ok(ProposalSummary {
        id: row.get(0)?,
        kind: ProposalKind::from_db(&row.get::<String>(1)?).ok_or(AppError::Internal)?,
        status,
        level: Level::from_db(&jkind).ok_or(AppError::Internal)?,
        jurisdiction: label(&jkind, row.get(4)?, row.get(5)?),
        title: row.get(6)?,
        summary: row.get(7)?,
        signature_count: row.get(8)?,
        signature_target: row.get(9)?,
        deadline,
    })
}

pub(crate) async fn summaries(
    conn: &Connection,
    tz: Tz,
    filter: &str,
    params: Vec<Value>,
) -> Result<Vec<ProposalSummary>, AppError> {
    query_all(conn, &format!("{SUMMARY_SQL} {filter}"), params)
        .await?
        .iter()
        .map(|r| summary_from(r, tz))
        .collect()
}

/// Public summary of one published proposal (drafts read as not found).
pub async fn summary_of(conn: &Connection, tz: Tz, id: &str) -> Result<ProposalSummary, AppError> {
    summaries(
        conn,
        tz,
        "WHERE p.id = ?1 AND p.status <> 'draft'",
        vec![id.into()],
    )
    .await?
    .pop()
    .ok_or(AppError::NotFound)
}

#[derive(Debug, Clone, Default)]
pub struct ListFilter {
    pub kind: Option<ProposalKind>,
    pub status: Option<ProposalStatus>,
    pub level: Option<Level>,
    pub page: u32,
}

impl ListFilter {
    /// Parses public query values; unknown values are rejected.
    pub fn parse(
        kind: Option<&str>,
        status: Option<&str>,
        level: Option<&str>,
        page: Option<u32>,
    ) -> Result<Self, AppError> {
        fn opt<T>(
            v: Option<&str>,
            field: &str,
            f: fn(&str) -> Option<T>,
        ) -> Result<Option<T>, AppError> {
            match v.filter(|v| !v.is_empty()) {
                None => Ok(None),
                Some(v) => f(v)
                    .map(Some)
                    .ok_or_else(|| AppError::validation(field, t("proposals.error.filter"))),
            }
        }
        let status = opt(status, "status", ProposalStatus::from_db)?;
        if status == Some(ProposalStatus::Draft) {
            return Err(AppError::validation("status", t("proposals.error.filter")));
        }
        Ok(Self {
            kind: opt(kind, "kind", ProposalKind::from_db)?,
            status,
            level: opt(level, "level", Level::from_db)?,
            page: page.unwrap_or(1).clamp(1, 10_000),
        })
    }
}

/// Public paginated listing. Drafts are never listed; archived only on request.
pub async fn list(
    conn: &Connection,
    tz: Tz,
    filter: &ListFilter,
) -> Result<ProposalPage, AppError> {
    let mut clauses = vec!["p.status <> 'draft'".to_owned()];
    let mut params: Vec<Value> = Vec::new();
    match filter.status {
        Some(s) => {
            params.push(s.as_db().into());
            clauses.push(format!("p.status = ?{}", params.len()));
        }
        None => clauses.push("p.status <> 'archived'".into()),
    }
    if let Some(k) = filter.kind {
        params.push(k.as_db().into());
        clauses.push(format!("p.kind = ?{}", params.len()));
    }
    if let Some(l) = filter.level {
        params.push(l.as_db().into());
        clauses.push(format!("j.kind = ?{}", params.len()));
    }
    let where_sql = format!("WHERE {}", clauses.join(" AND "));
    let total: u64 = query_opt(
        conn,
        &format!("SELECT COUNT(*) FROM proposals p JOIN jurisdictions j ON j.id = p.jurisdiction_id {where_sql}"),
        params.clone(),
    )
    .await?
    .ok_or(AppError::Internal)?
    .get(0)?;
    let pages = total.div_ceil(u64::from(PAGE_SIZE)).max(1) as u32;
    let page = filter.page.clamp(1, pages);
    params.push(i64::from(PAGE_SIZE).into());
    params.push(i64::from((page - 1) * PAGE_SIZE).into());
    let n = params.len();
    let items = summaries(
        conn,
        tz,
        &format!(
            "{where_sql} ORDER BY p.published_at DESC, p.id LIMIT ?{} OFFSET ?{}",
            n - 1,
            n
        ),
        params,
    )
    .await?;
    Ok(ProposalPage {
        items,
        page,
        pages,
        total,
    })
}

async fn has_active_signature(
    conn: &Connection,
    proposal_id: &str,
    user_id: &str,
) -> Result<bool, AppError> {
    Ok(query_opt(
        conn,
        "SELECT 1 FROM signatures WHERE proposal_id = ?1 AND user_id = ?2 AND active = 1",
        (proposal_id, user_id),
    )
    .await?
    .is_some())
}

pub async fn in_snapshot(
    conn: &Connection,
    snapshot_id: &str,
    user_id: &str,
) -> Result<bool, AppError> {
    Ok(query_opt(
        conn,
        "SELECT 1 FROM electorate_members WHERE snapshot_id = ?1 AND user_id = ?2",
        (snapshot_id, user_id),
    )
    .await?
    .is_some())
}

/// Public detail; drafts are visible to their author only.
pub async fn detail(
    conn: &Connection,
    flags: &Flags,
    now: DateTime<Utc>,
    tz: Tz,
    viewer: Option<&Actor>,
    id: &str,
) -> Result<ProposalDetail, AppError> {
    let p = load(conn, id).await?.ok_or(AppError::NotFound)?;
    let is_author = viewer.is_some_and(|a| a.user_id == p.author_id);
    if p.status == ProposalStatus::Draft && !is_author {
        return Err(AppError::NotFound);
    }
    let summary = summaries(conn, tz, "WHERE p.id = ?1", vec![id.into()])
        .await?
        .pop()
        .ok_or(AppError::NotFound)?;
    let author: String = query_opt(
        conn,
        "SELECT name FROM users WHERE id = ?1",
        [p.author_id.as_str()],
    )
    .await?
    .ok_or(AppError::Internal)?
    .get(0)?;
    let mut deadlines = Vec::new();
    for (phase, at) in [
        (ProposalStatus::CollectingSignatures, &p.collect_ends_at),
        (ProposalStatus::InDebate, &p.debate_ends_at),
        (ProposalStatus::InVoting, &p.vote_ends_at),
    ] {
        if let Some(at) = at {
            deadlines.push(Deadline {
                phase,
                at: local(at, tz),
                passed: parse_ts(at).is_some_and(|d| d <= now),
            });
        }
    }
    let setup_version = match &p.setup_version_id {
        Some(sid) => setup::get(conn, sid).await?.and_then(|s| s.version),
        None => None,
    };
    let proposed_setup = match &p.proposed_setup_id {
        Some(sid) => setup::get(conn, sid).await?.map(|s| s.params),
        None => None,
    };
    let viewer = match viewer {
        Some(actor) => {
            let collecting = p.status == ProposalStatus::CollectingSignatures
                && p.collect_ends_at
                    .as_deref()
                    .and_then(parse_ts)
                    .is_some_and(|d| now < d);
            let eligible = actor.can(Permission::ProposalSign)
                && actor.participates_in(&p.jurisdiction_id)
                && match &p.collection_snapshot_id {
                    Some(s) => in_snapshot(conn, s, &actor.user_id).await?,
                    None => true,
                };
            let has_signed = has_active_signature(conn, id, &actor.user_id).await?;
            let signatures_on = flags.is_enabled(flags::SIGNATURES);
            Some(ViewerState {
                is_author,
                eligible,
                has_signed,
                can_sign: signatures_on && collecting && eligible && !has_signed,
                can_withdraw_signature: signatures_on && collecting && has_signed,
                can_edit: is_author && p.status == ProposalStatus::Draft,
                can_publish: is_author
                    && p.status == ProposalStatus::Draft
                    && flags.is_enabled(flags::PROPOSALS),
                can_withdraw: is_author
                    && matches!(
                        p.status,
                        ProposalStatus::Draft | ProposalStatus::CollectingSignatures
                    ),
                can_archive: actor.can(Permission::ProposalArchive) && p.status.is_terminal(),
            })
        }
        None => None,
    };
    Ok(ProposalDetail {
        summary,
        body: p.body,
        text_hash: p.text_hash,
        author,
        published_at: p.published_at.as_deref().map(|at| local(at, tz)),
        deadlines,
        setup_version,
        archive_reason: p.archive_reason,
        proposed_setup,
        viewer,
    })
}

/// Signer count. Names stay private until a public-profile setting exists.
pub async fn signers(conn: &Connection, id: &str) -> Result<Signers, AppError> {
    let p = load(conn, id).await?.ok_or(AppError::NotFound)?;
    if p.status == ProposalStatus::Draft {
        return Err(AppError::NotFound);
    }
    Ok(Signers {
        count: p.signature_count,
        public_names: Vec::new(),
        visibility_available: false,
    })
}

pub async fn dashboard(
    conn: &Connection,
    tz: Tz,
    viewer: Option<&Actor>,
) -> Result<Dashboard, AppError> {
    let by_status = |status: ProposalStatus| {
        summaries(
            conn,
            tz,
            "WHERE p.status = ?1 ORDER BY p.published_at DESC LIMIT 10",
            vec![status.as_db().into()],
        )
    };
    let (mine, signed) = match viewer {
        Some(a) => (
            Some(
                summaries(
                    conn,
                    tz,
                    "WHERE p.author_id = ?1 ORDER BY p.updated_at DESC LIMIT 50",
                    vec![a.user_id.as_str().into()],
                )
                .await?,
            ),
            Some(
                summaries(
                    conn,
                    tz,
                    "JOIN signatures s ON s.proposal_id = p.id WHERE s.user_id = ?1 AND s.active = 1 ORDER BY s.updated_at DESC LIMIT 50",
                    vec![a.user_id.as_str().into()],
                )
                .await?,
            ),
        ),
        None => (None, None),
    };
    Ok(Dashboard {
        voting: by_status(ProposalStatus::InVoting).await?,
        debate: by_status(ProposalStatus::InDebate).await?,
        collecting: by_status(ProposalStatus::CollectingSignatures).await?,
        mine,
        signed,
    })
}

/// Markdown export of a published proposal with its text hash.
pub async fn export_markdown(conn: &Connection, tz: Tz, id: &str) -> Result<String, AppError> {
    let p = load(conn, id).await?.ok_or(AppError::NotFound)?;
    let hash = p.text_hash.clone().ok_or(AppError::NotFound)?;
    let jurisdiction = jurisdiction_label(conn, &p.jurisdiction_id).await?;
    Ok(format!(
        "# {title}\n\n- {k_kind}: {kind}\n- {k_jur}: {jurisdiction}\n- {k_status}: {status}\n- {k_pub}: {published}\n- ID: {id}\n\n## {k_summary}\n\n{summary}\n\n## {k_body}\n\n{body}\n\n---\n\nSHA-256: `{hash}`\n\n{k_hash}\n",
        title = p.title,
        k_kind = t("proposals.field.kind"),
        kind = t(p.kind.label_key()),
        k_jur = t("proposals.field.jurisdiction"),
        k_status = t("proposals.field.status"),
        status = t(p.status.label_key()),
        k_pub = t("proposals.field.published_at"),
        published = p
            .published_at
            .as_deref()
            .map(|at| local(at, tz))
            .unwrap_or_default(),
        id = p.id,
        k_summary = t("proposals.field.summary"),
        summary = p.summary,
        k_body = t("proposals.field.body"),
        body = p.body,
        k_hash = t("proposals.export.hash_rule"),
    ))
}
