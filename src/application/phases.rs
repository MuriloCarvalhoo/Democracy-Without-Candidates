//! Phase transitions with persisted UTC deadlines, electorate snapshots and the
//! voting hand-off. `advance_due` is idempotent: the background ticker and
//! on-read checks both call it, so restarts resume from the stored instants.

use std::{sync::Arc, time::Duration as StdDuration};

use chrono::{DateTime, Duration, Utc};
use serde_json::json;
use turso::Connection;

use super::{
    actor::Actor,
    audit::{self, Event, Visibility},
    notify::{Notice, notify},
    proposals::{self, ProposalRow},
    setup::{self, SetupVersion},
    voting,
};
use crate::{
    domain::{
        identity::Permission,
        proposals::{
            ElectoralError, ProposalKind, ProposalStatus, ResultCalc, Tally, can_transition,
            compute_result,
        },
        setup::SetupParams,
    },
    error::AppError,
    i18n::t,
    infrastructure::{
        clock::{Clock, ts},
        crypto::new_id,
        db::{Db, query_all, query_opt},
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Collection,
    Voting,
}

impl Phase {
    fn as_db(self) -> &'static str {
        match self {
            Self::Collection => "collection",
            Self::Voting => "voting",
        }
    }
}

const ELIGIBLE: &str = "FROM users u
    WHERE u.status = 'active'
      AND (EXISTS (SELECT 1 FROM iam_user_policies up WHERE up.user_id = u.id AND up.policy_id = 'pol-cidadao')
           OR EXISTS (SELECT 1 FROM iam_user_groups ug JOIN iam_group_policies gp ON gp.group_id = ug.group_id
                      WHERE ug.user_id = u.id AND gp.policy_id = 'pol-cidadao'))
      AND (u.municipality_id = ?1 OR u.state_id = ?1 OR u.country_id = ?1)";

/// Number of eligible accounts of a jurisdiction right now.
pub async fn electorate_count(conn: &Connection, jurisdiction_id: &str) -> Result<u64, AppError> {
    Ok(query_opt(
        conn,
        &format!("SELECT COUNT(*) {ELIGIBLE}"),
        [jurisdiction_id],
    )
    .await?
    .ok_or_else(|| AppError::internal("missing or invalid stored value"))?
    .get(0)?)
}

/// Materializes the eligible accounts of a jurisdiction (active accounts holding
/// the managed `Cidadao` policy, directly or through a group,
/// whose municipality/state/country matches) for one proposal phase. Immutable.
/// An empty electorate cannot open a phase.
#[tracing::instrument(level = "debug", skip_all, fields(proposal_id = %proposal_id, phase = ?phase, jurisdiction_id = %jurisdiction_id, setup_version_id = %setup_version_id), err(level = "debug"))]
pub async fn snapshot(
    tx: &Connection,
    now: DateTime<Utc>,
    proposal_id: &str,
    phase: Phase,
    jurisdiction_id: &str,
    setup_version_id: &str,
) -> Result<(String, u64), AppError> {
    let count = electorate_count(tx, jurisdiction_id).await?;
    if count == 0 {
        return Err(AppError::Conflict(
            t("proposals.error.empty_electorate").into(),
        ));
    }
    let id = new_id();
    tx.execute(
        "INSERT INTO electorate_snapshots (id, proposal_id, phase, jurisdiction_id, setup_version_id, member_count, taken_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        (id.as_str(), proposal_id, phase.as_db(), jurisdiction_id, setup_version_id, count, ts(now)),
    )
    .await?;
    let inserted = tx
        .execute(
            &format!(
                "INSERT INTO electorate_members (snapshot_id, user_id) SELECT ?2, u.id {ELIGIBLE}"
            ),
            (jurisdiction_id, id.as_str()),
        )
        .await?;
    if inserted != count {
        return Err(AppError::internal(format_args!(
            "electorate snapshot inserted {inserted} of {count} members"
        )));
    }
    Ok((id, count))
}

/// Records a phase change: public audit event and a notification to the author.
async fn record(
    tx: &Connection,
    now: DateTime<Utc>,
    p: &ProposalRow,
    to: ProposalStatus,
    extra: serde_json::Value,
) -> Result<(), AppError> {
    audit::append(
        tx,
        now,
        Event {
            kind: "proposal.phase_changed",
            visibility: Visibility::Public,
            actor_ref: None,
            subject: Some(&p.id),
            payload: json!({ "from": p.status.as_db(), "to": to.as_db(), "detail": extra }),
        },
    )
    .await?;
    let link = format!("/propostas/{}", p.id);
    let title = format!("{}: {}", t("proposals.notify.phase"), t(to.label_key()));
    notify(
        tx,
        now,
        &p.author_id,
        Notice {
            event: "proposal.phase_changed",
            title: &title,
            body: &p.title,
            link: Some(&link),
        },
    )
    .await
}

/// Closes collection after the threshold is reached and opens the next phase:
/// debate (normal law, setup change) or voting (urgent law).
#[tracing::instrument(level = "debug", skip_all, err(level = "debug"))]
pub async fn close_collection(
    tx: &Connection,
    now: DateTime<Utc>,
    p: &ProposalRow,
) -> Result<ProposalStatus, AppError> {
    let next = p
        .kind
        .after_collection()
        .ok_or_else(|| AppError::internal("missing or invalid stored value"))?;
    match next {
        ProposalStatus::InDebate => {
            let setup_id = p
                .setup_version_id
                .as_deref()
                .ok_or_else(|| AppError::internal("missing or invalid stored value"))?;
            let setup = setup::get(tx, setup_id)
                .await?
                .ok_or_else(|| AppError::internal("missing or invalid stored value"))?;
            let days = setup
                .params
                .rules(p.kind)
                .debate_days
                .ok_or_else(|| AppError::internal("missing or invalid stored value"))?;
            let ends = ts(now + Duration::days(days.into()));
            let changed = tx
                .execute(
                    "UPDATE proposals SET status = 'in_debate', debate_started_at = ?2, debate_ends_at = ?3, updated_at = ?2
                     WHERE id = ?1 AND status = 'collecting_signatures'",
                    (p.id.as_str(), ts(now), ends.as_str()),
                )
                .await?;
            if changed != 1 {
                return Err(AppError::PhaseClosed);
            }
            record(
                tx,
                now,
                p,
                next,
                json!({ "debate_ends_at": ends, "signature_count": p.signature_count }),
            )
            .await?;
        }
        _ => open_voting(tx, now, p).await?,
    }
    Ok(next)
}

/// Opens voting with the setup effective now and a fresh electorate snapshot.
/// Notifies the jurisdiction (`voting::on_opened`) in the same transaction.
#[tracing::instrument(level = "debug", skip_all, err(level = "debug"))]
pub async fn open_voting(
    tx: &Connection,
    now: DateTime<Utc>,
    p: &ProposalRow,
) -> Result<(), AppError> {
    if !can_transition(p.kind, p.status, ProposalStatus::InVoting) {
        return Err(AppError::PhaseClosed);
    }
    let setup = setup::effective_setup(tx, &p.jurisdiction_id)
        .await?
        .ok_or_else(|| AppError::Conflict(t("proposals.error.no_setup").into()))?;
    open_voting_with(tx, now, p, &setup.id, &setup.params).await
}

async fn open_voting_with(
    tx: &Connection,
    now: DateTime<Utc>,
    p: &ProposalRow,
    setup_id: &str,
    params: &SetupParams,
) -> Result<(), AppError> {
    let (snapshot_id, electorate) =
        snapshot(tx, now, &p.id, Phase::Voting, &p.jurisdiction_id, setup_id).await?;
    let ends_at = now + Duration::days(params.rules(p.kind).vote_days.into());
    let ends = ts(ends_at);
    let changed = tx
        .execute(
            "UPDATE proposals SET status = 'in_voting', vote_setup_version_id = ?3, vote_snapshot_id = ?4,
               vote_started_at = ?5, vote_ends_at = ?6, updated_at = ?5
             WHERE id = ?1 AND status = ?2",
            (p.id.as_str(), p.status.as_db(), setup_id, snapshot_id.as_str(), ts(now), ends.as_str()),
        )
        .await?;
    if changed != 1 {
        return Err(AppError::PhaseClosed);
    }
    record(
        tx,
        now,
        p,
        ProposalStatus::InVoting,
        json!({ "vote_ends_at": ends, "electorate": electorate }),
    )
    .await?;
    voting::on_opened(tx, now, p, ends_at).await
}

/// Ids whose current phase deadline has passed.
async fn due(conn: &Connection, now: &str) -> Result<Vec<(String, ProposalStatus)>, AppError> {
    let rows = query_all(
        conn,
        "SELECT id, status FROM proposals
         WHERE (status = 'collecting_signatures' AND collect_ends_at <= ?1)
            OR (status = 'in_debate' AND debate_ends_at <= ?1)
         ORDER BY id",
        [now],
    )
    .await?;
    rows.iter()
        .map(|r| {
            Ok((
                r.get(0)?,
                ProposalStatus::from_db(&r.get::<String>(1)?)
                    .ok_or_else(|| AppError::internal("missing or invalid stored value"))?,
            ))
        })
        .collect()
}

/// Applies every due transition, one transaction per proposal, re-checking the
/// state inside the transaction. Collection deadline → `expired`; debate end →
/// `in_voting`. Returns the applied `(id, new status)` pairs.
///
/// Votes past `vote_ends_at` are then closed by `voting::close_due_votes`
/// (tally from ballots + `close_vote`), also one transaction each.
#[tracing::instrument(level = "debug", skip_all, err(level = "debug"))]
pub async fn advance_due(
    db: &Db,
    now: DateTime<Utc>,
) -> Result<Vec<(String, ProposalStatus)>, AppError> {
    let now_ts = ts(now);
    let candidates = due(&db.read().await?, &now_ts).await?;
    let mut applied = Vec::new();
    for (id, _) in candidates {
        let tx = db.write().await?;
        let Some(p) = proposals::load(&tx, &id).await? else {
            continue;
        };
        let deadline = match p.status {
            ProposalStatus::CollectingSignatures => p.collect_ends_at.as_deref(),
            ProposalStatus::InDebate => p.debate_ends_at.as_deref(),
            _ => None,
        };
        if deadline.is_none_or(|d| d > now_ts.as_str()) {
            continue;
        }
        let to = match p.status {
            ProposalStatus::CollectingSignatures => {
                tx.execute(
                    "UPDATE proposals SET status = 'expired', closed_at = ?2, updated_at = ?2 WHERE id = ?1 AND status = 'collecting_signatures'",
                    (id.as_str(), now_ts.as_str()),
                )
                .await?;
                record(&tx, now, &p, ProposalStatus::Expired, json!({ "signature_count": p.signature_count, "signature_target": p.signature_target })).await?;
                reject_package(&tx, &p).await?;
                ProposalStatus::Expired
            }
            _ => match open_voting(&tx, now, &p).await {
                Ok(()) => ProposalStatus::InVoting,
                // Nobody eligible anymore: the election cannot open; keep history.
                Err(AppError::Conflict(_)) => {
                    tx.execute(
                        "UPDATE proposals SET status = 'archived', archived_at = ?2, archive_reason = 'empty_electorate', closed_at = ?2, updated_at = ?2
                         WHERE id = ?1 AND status = 'in_debate'",
                        (id.as_str(), now_ts.as_str()),
                    )
                    .await?;
                    record(
                        &tx,
                        now,
                        &p,
                        ProposalStatus::Archived,
                        json!({ "reason": "empty_electorate" }),
                    )
                    .await?;
                    reject_package(&tx, &p).await?;
                    ProposalStatus::Archived
                }
                Err(e) => return Err(e),
            },
        };
        tx.commit().await?;
        applied.push((id, to));
    }
    applied.extend(voting::close_due_votes(db, now).await?);
    Ok(applied)
}

async fn reject_package(tx: &Connection, p: &ProposalRow) -> Result<(), AppError> {
    if let Some(sid) = &p.proposed_setup_id {
        tx.execute(
            "UPDATE setup_versions SET status = 'rejected' WHERE id = ?1 AND status = 'proposed'",
            [sid.as_str()],
        )
        .await?;
    }
    Ok(())
}

/// Votes whose deadline passed and still await closing by the voting feature.
#[tracing::instrument(level = "debug", skip_all, err(level = "debug"))]
pub async fn due_votes(conn: &Connection, now: DateTime<Utc>) -> Result<Vec<String>, AppError> {
    query_all(
        conn,
        "SELECT id FROM proposals WHERE status = 'in_voting' AND vote_ends_at <= ?1 ORDER BY id",
        [ts(now)],
    )
    .await?
    .iter()
    .map(|r| Ok(r.get(0)?))
    .collect()
}

/// Rules governing an open vote: `(electorate size, quorum bp, majority bp)`,
/// from the vote snapshot and the setup version captured at voting opening.
#[tracing::instrument(level = "debug", skip_all, err(level = "debug"))]
pub async fn vote_rules(conn: &Connection, p: &ProposalRow) -> Result<(u64, u32, u32), AppError> {
    let snapshot = p.vote_snapshot_id.as_deref().ok_or(AppError::PhaseClosed)?;
    let setup_id = p
        .vote_setup_version_id
        .as_deref()
        .ok_or(AppError::PhaseClosed)?;
    let electorate: u64 = query_opt(
        conn,
        "SELECT member_count FROM electorate_snapshots WHERE id = ?1",
        [snapshot],
    )
    .await?
    .ok_or_else(|| AppError::internal("missing or invalid stored value"))?
    .get(0)?;
    let setup: SetupVersion = setup::get(conn, setup_id)
        .await?
        .ok_or_else(|| AppError::internal("missing or invalid stored value"))?;
    let rules = setup.params.rules(p.kind);
    Ok((electorate, rules.quorum_bp, rules.majority_bp))
}

/// Closes a vote with the tally computed by the voting feature, inside its write
/// transaction: applies `compute_result`, stores the terminal status, audits,
/// notifies the author and, for setup plebiscites/meta-laws, adopts or rejects
/// the proposed package (effective only for elections opened afterwards).
#[tracing::instrument(level = "debug", skip_all, fields(proposal_id = %proposal_id), err(level = "debug"))]
pub async fn close_vote(
    tx: &Connection,
    now: DateTime<Utc>,
    proposal_id: &str,
    tally: Tally,
) -> Result<ResultCalc, AppError> {
    let p = proposals::load(tx, proposal_id)
        .await?
        .ok_or(AppError::NotFound)?;
    if p.status != ProposalStatus::InVoting {
        return Err(AppError::PhaseClosed);
    }
    let (electorate, quorum_bp, majority_bp) = vote_rules(tx, &p).await?;
    let calc = compute_result(tally, electorate, quorum_bp, majority_bp).map_err(|e| match e {
        ElectoralError::EmptyElectorate | ElectoralError::TallyExceedsElectorate => {
            AppError::internal(format_args!("vote result: {e:?}"))
        }
    })?;
    let to = calc.outcome.status();
    let changed = tx
        .execute(
            "UPDATE proposals SET status = ?2, closed_at = ?3, updated_at = ?3 WHERE id = ?1 AND status = 'in_voting'",
            (proposal_id, to.as_db(), ts(now)),
        )
        .await?;
    if changed != 1 {
        return Err(AppError::PhaseClosed);
    }
    if let Some(sid) = &p.proposed_setup_id {
        if to == ProposalStatus::Approved {
            setup::adopt_proposed(tx, now, sid, Some(proposal_id)).await?;
        } else {
            reject_package(tx, &p).await?;
        }
    }
    record(
        tx,
        now,
        &p,
        to,
        json!({ "tally": tally, "electorate": electorate, "result": calc }),
    )
    .await?;
    Ok(calc)
}

/// Starts the initial setup plebiscite for the national jurisdiction with a
/// proposed package (normally `setup::SUGGESTED`): the country admin only starts
/// it; adoption happens through `close_vote` if approved. Opens voting directly.
#[tracing::instrument(level = "debug", skip_all, fields(actor = %actor.user_id), err(level = "debug"))]
pub async fn start_initial_setup(
    db: &Db,
    now: DateTime<Utc>,
    actor: &Actor,
    params: &SetupParams,
    title: &str,
    body: &str,
) -> Result<String, AppError> {
    actor.require(Permission::SetupStart)?;
    let tx = db.write().await?;
    let national = actor.country_id.clone();
    if setup::effective_setup(&tx, &national).await?.is_some() {
        return Err(AppError::Conflict(t("proposals.error.setup_exists").into()));
    }
    let open: u32 = query_opt(
        &tx,
        "SELECT COUNT(*) FROM proposals WHERE kind = 'initial_setup' AND status = 'in_voting'",
        (),
    )
    .await?
    .ok_or_else(|| AppError::internal("missing or invalid stored value"))?
    .get(0)?;
    if open > 0 {
        return Err(AppError::Conflict(
            t("proposals.error.setup_in_progress").into(),
        ));
    }
    let synthetic = query_opt(
        &tx,
        "SELECT synthetic FROM users WHERE id = ?1",
        [actor.user_id.as_str()],
    )
    .await?
    .map(|r| r.get::<i64>(0))
    .transpose()?
        == Some(1);
    let package = setup::insert_proposed(&tx, now, &national, params, synthetic).await?;
    let id = new_id();
    let summary = t("proposals.initial_setup.summary");
    tx.execute(
        "INSERT INTO proposals (id, kind, jurisdiction_id, author_id, title, summary, body, normalized_title, normalized_body_hash,
           text_hash, status, proposed_setup_id, created_at, updated_at, published_at)
         VALUES (?1, 'initial_setup', ?2, ?3, ?4, ?5, ?6, '', '', ?7, 'draft', ?8, ?9, ?9, NULL)",
        (
            id.as_str(),
            national.as_str(),
            actor.user_id.as_str(),
            title,
            summary,
            body,
            proposals::text_hash(title, summary, body),
            package.as_str(),
            ts(now),
        ),
    )
    .await?;
    // Draft → voting: published_at marks the start; the row never stays a draft.
    let row = proposals::load(&tx, &id)
        .await?
        .ok_or_else(|| AppError::internal("missing or invalid stored value"))?;
    let (snapshot_id, electorate) =
        snapshot(&tx, now, &id, Phase::Voting, &national, &package).await?;
    let ends_at = now + Duration::days(params.rules(ProposalKind::InitialSetup).vote_days.into());
    let ends = ts(ends_at);
    tx.execute(
        "UPDATE proposals SET status = 'in_voting', published_at = ?2, vote_setup_version_id = ?3, vote_snapshot_id = ?4,
           vote_started_at = ?2, vote_ends_at = ?5, updated_at = ?2 WHERE id = ?1 AND status = 'draft'",
        (id.as_str(), ts(now), package.as_str(), snapshot_id.as_str(), ends.as_str()),
    )
    .await?;
    record(
        &tx,
        now,
        &row,
        ProposalStatus::InVoting,
        json!({ "vote_ends_at": ends, "electorate": electorate }),
    )
    .await?;
    voting::on_opened(&tx, now, &row, ends_at).await?;
    tx.commit().await?;
    Ok(id)
}

/// Background ticker: applies due transitions every `every` using the injected
/// clock. Errors are logged and retried on the next tick.
pub async fn run_ticker(db: Db, clock: Arc<dyn Clock>, every: StdDuration) {
    let mut interval = tokio::time::interval(every);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        interval.tick().await;
        match advance_due(&db, clock.now()).await {
            Ok(applied) if !applied.is_empty() => {
                tracing::info!(?applied, "proposal deadlines applied")
            }
            Ok(_) => {}
            Err(error) => tracing::warn!(%error, "proposal deadline check failed"),
        }
    }
}
