//! Demonstrative PUBLIC vote (flag `voting`): one ballot per account and
//! proposal, linked to the account with explicit consent. No secret ballot, no
//! receipt with cryptographic guarantees and no electoral validity: secret voting
//! (`voting.private`) is blocked until a validated protocol exists.
//!
//! Casting and closing run in the single serialized write transaction, so a
//! ballot either commits before the closing transaction (and is counted) or
//! observes the closed phase and is rejected with `PhaseClosed`.
//!
//! Audit: `vote.cast` is public with the running count only (no account, no
//! choice); the ballot row itself is linked to the account because the vote is
//! public by consent. No partial counts are published before closing.

use chrono::{DateTime, Duration, Utc};
use serde_json::json;
use turso::Connection;

use super::{
    actor::Actor,
    audit::{self, Event, Visibility},
    idempotency::{self, Key},
    notify::{self, Audience, Notice},
    phases,
    proposals::{self, Ctx, ProposalRow},
    reauth, setup,
};
use crate::{
    domain::{
        identity::Permission,
        notification::Category,
        proposals::{Outcome, ProposalKind, ProposalStatus, RejectReason, Tally},
        voting::{
            Choice, is_setup_vote, reauth_action,
            view::{
                CastConfirmation, OwnBallot, ResultPage, ResultState, ResultView, VotePage,
                VoteStatus, VoterState,
            },
        },
    },
    error::AppError,
    i18n::t,
    infrastructure::{
        clock::{parse_ts, ts},
        crypto::new_id,
        db::{Db, is_constraint, query_all, query_opt},
        flags::{self, Flags},
    },
};

const CAST_ACTION: &str = "proposal.vote";

fn permission(kind: ProposalKind) -> Permission {
    if is_setup_vote(kind) {
        Permission::SetupVote
    } else {
        Permission::ProposalVote
    }
}

fn is_open(p: &ProposalRow, now: DateTime<Utc>) -> bool {
    p.status == ProposalStatus::InVoting
        && p.vote_ends_at
            .as_deref()
            .is_some_and(|d| ts(now).as_str() < d)
}

async fn own_ballot(
    conn: &Connection,
    tz: chrono_tz::Tz,
    proposal_id: &str,
    user_id: &str,
) -> Result<Option<OwnBallot>, AppError> {
    let Some(row) = query_opt(
        conn,
        "SELECT choice, cast_at FROM ballots WHERE proposal_id = ?1 AND user_id = ?2",
        (proposal_id, user_id),
    )
    .await?
    else {
        return Ok(None);
    };
    Ok(Some(OwnBallot {
        choice: Choice::from_db(&row.get::<String>(0)?)
            .ok_or_else(|| AppError::internal("missing or invalid stored value"))?,
        cast_at: proposals::local(&row.get::<String>(1)?, tz),
    }))
}

/// Counts of accepted ballots by choice.
#[tracing::instrument(level = "debug", skip_all, fields(proposal_id = %proposal_id), err(level = "debug"))]
pub async fn tally(conn: &Connection, proposal_id: &str) -> Result<Tally, AppError> {
    let mut t = Tally::default();
    for row in query_all(
        conn,
        "SELECT choice, COUNT(*) FROM ballots WHERE proposal_id = ?1 GROUP BY choice",
        [proposal_id],
    )
    .await?
    {
        let n: u64 = row.get(1)?;
        match Choice::from_db(&row.get::<String>(0)?)
            .ok_or_else(|| AppError::internal("missing or invalid stored value"))?
        {
            Choice::Yes => t.yes = n,
            Choice::No => t.no = n,
            Choice::Abstain => t.abstain = n,
        }
    }
    Ok(t)
}

/// Casts a public ballot. Requires the `voting` and `proposals` flags, explicit
/// consent to a public vote linked to the account, an open vote (`now <
/// vote_ends_at` inside the write transaction), membership in the voting
/// electorate snapshot and a reauth grant (`proposal:vote`, or `setup:vote` for
/// setup plebiscites/meta-laws) consumed in the same transaction.
///
/// Idempotency: the same key and payload return the stored confirmation; the same
/// key with another payload is a conflict; a new attempt after an accepted
/// ballot is a conflict ("voto já registrado").
#[tracing::instrument(level = "debug", skip_all, fields(actor = %actor.user_id, id = %id, public_consent = %public_consent), err(level = "debug"))]
pub async fn cast(
    ctx: &Ctx<'_>,
    actor: &Actor,
    id: &str,
    choice: Choice,
    public_consent: bool,
    idempotency_key: Option<&str>,
) -> Result<CastConfirmation, AppError> {
    ctx.flags.require(flags::PROPOSALS)?;
    ctx.flags.require(flags::VOTING)?;
    if !public_consent {
        return Err(AppError::validation(
            "public_consent",
            t("voting.error.consent"),
        ));
    }
    let key = idempotency::parse(idempotency_key)?.map(|key| Key {
        user_id: &actor.user_id,
        key,
        action: CAST_ACTION,
        request_hash: idempotency::request_hash(&[id, choice.as_db(), "public"]),
    });
    let (tx, ctx) = ctx.write().await?;
    if let Some(key) = &key
        && let Some(previous) = idempotency::lookup(&tx, key).await?
    {
        return Ok(previous);
    }
    let p = proposals::load(&tx, id).await?.ok_or(AppError::NotFound)?;
    if p.status == ProposalStatus::Draft {
        return Err(AppError::NotFound);
    }
    actor.require(permission(p.kind))?;
    if !is_open(&p, ctx.now) {
        return Err(AppError::PhaseClosed);
    }
    let snapshot = p
        .vote_snapshot_id
        .clone()
        .ok_or_else(|| AppError::internal("missing or invalid stored value"))?;
    if !proposals::in_snapshot(&tx, &snapshot, &actor.user_id).await? {
        return Err(AppError::Forbidden);
    }
    if query_opt(
        &tx,
        "SELECT 1 FROM ballots WHERE proposal_id = ?1 AND user_id = ?2",
        (id, actor.user_id.as_str()),
    )
    .await?
    .is_some()
    {
        return Err(AppError::Conflict(t("voting.error.already_voted").into()));
    }
    reauth::consume(&tx, ctx.now, &actor.session_hash, reauth_action(p.kind), id).await?;
    let now = ts(ctx.now);
    tx.execute(
        "INSERT INTO ballots (id, proposal_id, user_id, snapshot_id, choice, visibility, consented_at, cast_at)
         VALUES (?1, ?2, ?3, ?4, ?5, 'public', ?6, ?6)",
        (new_id(), id, actor.user_id.as_str(), snapshot.as_str(), choice.as_db(), now.as_str()),
    )
    .await
    .map_err(|e| {
        if is_constraint(&e) {
            AppError::Conflict(t("voting.error.already_voted").into())
        } else {
            e.into()
        }
    })?;
    let cast: u64 = query_opt(
        &tx,
        "SELECT COUNT(*) FROM ballots WHERE proposal_id = ?1",
        [id],
    )
    .await?
    .ok_or_else(|| AppError::internal("missing or invalid stored value"))?
    .get(0)?;
    audit::append(
        &tx,
        ctx.now,
        Event {
            kind: "vote.cast",
            visibility: Visibility::Public,
            actor_ref: None,
            subject: Some(id),
            payload: json!({ "votes_cast": cast, "visibility": "public" }),
        },
    )
    .await?;
    let result = CastConfirmation {
        proposal_id: id.to_owned(),
        choice,
        cast_at: proposals::local(&now, ctx.tz),
    };
    if let Some(key) = &key {
        idempotency::store(&tx, ctx.now, key, &result).await?;
    }
    tx.commit().await?;
    Ok(result)
}

/// Own ballot plus whether an attempt key was stored with a result: lets the
/// client resolve an unknown outcome (timeout) without casting again.
#[tracing::instrument(level = "debug", skip_all, fields(actor = %actor.user_id, id = %id), err(level = "debug"))]
pub async fn status(
    conn: &Connection,
    tz: chrono_tz::Tz,
    actor: &Actor,
    id: &str,
    idempotency_key: Option<&str>,
) -> Result<VoteStatus, AppError> {
    let attempt_recorded = match idempotency::parse(idempotency_key)? {
        Some(key) => query_opt(
            conn,
            "SELECT 1 FROM idempotency_keys WHERE user_id = ?1 AND key = ?2 AND action = ?3",
            (actor.user_id.as_str(), key, CAST_ACTION),
        )
        .await?
        .is_some(),
        None => false,
    };
    Ok(VoteStatus {
        ballot: own_ballot(conn, tz, id, &actor.user_id).await?,
        attempt_recorded,
    })
}

/// Notifications when a vote opens, inside the opening transaction: the
/// jurisdiction is told now and reminded 24 h before the deadline (delivered by
/// the scheduled-notification ticker to every active participant).
#[tracing::instrument(level = "debug", skip_all, err(level = "debug"))]
pub async fn on_opened(
    tx: &Connection,
    now: DateTime<Utc>,
    p: &ProposalRow,
    ends: DateTime<Utc>,
) -> Result<(), AppError> {
    let link = format!("/propostas/{}/votar", p.id);
    let audience = Audience::Jurisdiction(&p.jurisdiction_id);
    notify::schedule(
        tx,
        now,
        &format!("vote-open:{}", p.id),
        audience,
        Notice {
            event: Category::VotingOpened.as_db(),
            title: t("voting.notify.opened"),
            body: &p.title,
            link: Some(&link),
        },
        now,
    )
    .await?;
    let remind_at = ends - Duration::hours(24);
    if remind_at > now {
        notify::schedule(
            tx,
            now,
            &format!("vote-24h:{}", p.id),
            audience,
            Notice {
                event: Category::DeadlineReminder.as_db(),
                title: t("voting.notify.reminder"),
                body: &p.title,
                link: Some(&link),
            },
            remind_at,
        )
        .await?;
    }
    Ok(())
}

fn outcome_db(o: Outcome) -> (&'static str, Option<&'static str>) {
    match o {
        Outcome::Approved => ("approved", None),
        Outcome::QuorumNotReached => ("quorum_not_reached", None),
        Outcome::Rejected(r) => (
            "rejected",
            Some(match r {
                RejectReason::NoValidVotes => "no_valid_votes",
                RejectReason::Tie => "tie",
                RejectReason::MajorityNotReached => "majority_not_reached",
            }),
        ),
    }
}

fn outcome_from_db(outcome: &str, reason: Option<&str>) -> Option<Outcome> {
    Some(match (outcome, reason) {
        ("approved", None) => Outcome::Approved,
        ("quorum_not_reached", None) => Outcome::QuorumNotReached,
        ("rejected", Some("no_valid_votes")) => Outcome::Rejected(RejectReason::NoValidVotes),
        ("rejected", Some("tie")) => Outcome::Rejected(RejectReason::Tie),
        ("rejected", Some("majority_not_reached")) => {
            Outcome::Rejected(RejectReason::MajorityNotReached)
        }
        _ => return None,
    })
}

/// Closes one vote whose deadline passed, inside the caller's transaction:
/// tally from ballots, `phases::close_vote`, immutable result row and result
/// notification to the jurisdiction. `None` when nothing is due (idempotent).
#[tracing::instrument(level = "debug", skip_all, fields(id = %id), err(level = "debug"))]
pub async fn close_one(
    tx: &Connection,
    now: DateTime<Utc>,
    id: &str,
) -> Result<Option<ProposalStatus>, AppError> {
    let Some(p) = proposals::load(tx, id).await? else {
        return Ok(None);
    };
    if p.status != ProposalStatus::InVoting
        || p.vote_ends_at
            .as_deref()
            .is_none_or(|d| d > ts(now).as_str())
    {
        return Ok(None);
    }
    let tally = tally(tx, id).await?;
    let (electorate, quorum_bp, majority_bp) = phases::vote_rules(tx, &p).await?;
    let calc = phases::close_vote(tx, now, id, tally).await?;
    let (outcome, reason) = outcome_db(calc.outcome);
    tx.execute(
        "INSERT INTO vote_results (proposal_id, snapshot_id, setup_version_id, electorate, yes_count, no_count, abstain_count,
           quorum_bp, majority_bp, quorum_required, majority_required, outcome, reason, closed_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
        (
            id,
            p.vote_snapshot_id.as_deref(),
            p.vote_setup_version_id.as_deref(),
            electorate,
            tally.yes,
            tally.no,
            tally.abstain,
            quorum_bp,
            majority_bp,
            calc.quorum_required,
            calc.majority_required,
            outcome,
            reason,
            ts(now),
        ),
    )
    .await?;
    let link = format!("/propostas/{id}/resultado");
    let title = format!(
        "{}: {}",
        t("voting.notify.result"),
        t(calc.outcome.status().label_key())
    );
    notify::schedule(
        tx,
        now,
        &format!("vote-result:{id}"),
        Audience::Jurisdiction(&p.jurisdiction_id),
        Notice {
            event: Category::ResultPublished.as_db(),
            title: &title,
            body: &p.title,
            link: Some(&link),
        },
        now,
    )
    .await?;
    notify::cancel_scheduled(tx, &format!("vote-24h:{id}")).await?;
    Ok(Some(calc.outcome.status()))
}

/// Closes every vote past its deadline, one transaction each. Idempotent and
/// restart-safe: state is re-checked inside each transaction.
#[tracing::instrument(level = "debug", skip_all, err(level = "debug"))]
pub async fn close_due_votes(
    db: &Db,
    now: DateTime<Utc>,
) -> Result<Vec<(String, ProposalStatus)>, AppError> {
    let ids = phases::due_votes(&db.read().await?, now).await?;
    let mut closed = Vec::new();
    for id in ids {
        let tx = db.write().await?;
        match close_one(&tx, now, &id).await? {
            Some(status) => {
                tx.commit().await?;
                closed.push((id, status));
            }
            None => tx.rollback().await?,
        }
    }
    Ok(closed)
}

/// Voting page model. Eligibility comes from the vote snapshot, never from the client.
#[tracing::instrument(level = "debug", skip_all, fields(demonstration = %demonstration, viewer = ?viewer.map(|a| a.user_id.as_str()), id = %id), err(level = "debug"))]
pub async fn page(
    conn: &Connection,
    flags: &Flags,
    now: DateTime<Utc>,
    tz: chrono_tz::Tz,
    demonstration: bool,
    viewer: Option<&Actor>,
    id: &str,
) -> Result<VotePage, AppError> {
    let p = proposals::load(conn, id).await?.ok_or(AppError::NotFound)?;
    let summary = proposals::summary_of(conn, tz, id).await?;
    let electorate = match &p.vote_snapshot_id {
        Some(s) => query_opt(
            conn,
            "SELECT member_count FROM electorate_snapshots WHERE id = ?1",
            [s.as_str()],
        )
        .await?
        .map(|r| r.get::<u64>(0))
        .transpose()?,
        None => None,
    };
    let viewer = match viewer {
        Some(actor) => {
            let eligible = actor.can(permission(p.kind))
                && match &p.vote_snapshot_id {
                    Some(s) => proposals::in_snapshot(conn, s, &actor.user_id).await?,
                    None => false,
                };
            Some(VoterState {
                eligible,
                ballot: own_ballot(conn, tz, id, &actor.user_id).await?,
            })
        }
        None => None,
    };
    Ok(VotePage {
        summary,
        open: is_open(&p, now),
        ends_at: p.vote_ends_at.as_deref().map(|d| proposals::local(d, tz)),
        electorate,
        voting_enabled: flags.is_enabled(flags::VOTING) && flags.is_enabled(flags::PROPOSALS),
        reauth_action: reauth_action(p.kind).to_owned(),
        demonstration,
        viewer,
    })
}

/// Public result. While voting is open only the deadline and electorate size
/// are shown (no partial counts, to avoid influencing voters).
#[tracing::instrument(level = "debug", skip_all, fields(demonstration = %demonstration, id = %id), err(level = "debug"))]
pub async fn result(
    conn: &Connection,
    now: DateTime<Utc>,
    tz: chrono_tz::Tz,
    demonstration: bool,
    id: &str,
) -> Result<ResultPage, AppError> {
    let p = proposals::load(conn, id).await?.ok_or(AppError::NotFound)?;
    let summary = proposals::summary_of(conn, tz, id).await?;
    let row = query_opt(
        conn,
        "SELECT snapshot_id, setup_version_id, electorate, yes_count, no_count, abstain_count, quorum_bp, majority_bp,
           quorum_required, majority_required, outcome, reason, closed_at FROM vote_results WHERE proposal_id = ?1",
        [id],
    )
    .await?;
    let state = match row {
        Some(r) => {
            let setup_version_id: String = r.get(1)?;
            let setup_version = setup::get(conn, &setup_version_id)
                .await?
                .and_then(|s| s.version);
            let package = match &p.proposed_setup_id {
                Some(sid) => Some(setup::get(conn, sid).await?.and_then(|s| {
                    matches!(s.status.as_str(), "adopted" | "superseded")
                        .then_some(s.version)
                        .flatten()
                })),
                None => None,
            };
            let outcome = outcome_from_db(
                &r.get::<String>(10)?,
                r.get::<Option<String>>(11)?.as_deref(),
            )
            .ok_or_else(|| AppError::internal("missing or invalid stored value"))?;
            ResultState::Closed(ResultView {
                snapshot_id: r.get(0)?,
                setup_version_id,
                setup_version,
                electorate: r.get(2)?,
                tally: Tally {
                    yes: r.get(3)?,
                    no: r.get(4)?,
                    abstain: r.get(5)?,
                },
                quorum_bp: r.get(6)?,
                majority_bp: r.get(7)?,
                quorum_required: r.get(8)?,
                majority_required: r.get(9)?,
                outcome,
                closed_at: proposals::local(&r.get::<String>(12)?, tz),
                package,
            })
        }
        None if p.status == ProposalStatus::InVoting => {
            let ends = p
                .vote_ends_at
                .clone()
                .ok_or_else(|| AppError::internal("missing or invalid stored value"))?;
            if parse_ts(&ends).is_some_and(|e| e <= now) {
                ResultState::Closing
            } else {
                let (electorate, ..) = phases::vote_rules(conn, &p).await?;
                ResultState::Open {
                    ends_at: proposals::local(&ends, tz),
                    electorate,
                }
            }
        }
        None => ResultState::NotOpened,
    };
    Ok(ResultPage {
        summary,
        state,
        demonstration,
    })
}

/// Read-only voting invariants for `check-db` (and the load harness).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct VotingCheck {
    pub ballots: i64,
    /// Accepted ballots of the most voted proposal (concentrated-vote load check).
    pub max_ballots_per_proposal: i64,
    pub duplicate_ballots: i64,
    /// Ballots whose account is not in the proposal's voting snapshot.
    pub ballots_outside_snapshot: i64,
    pub results: i64,
    /// Results whose counts differ from the stored ballots.
    pub results_mismatch: i64,
    /// Closed votes (approved/rejected/quorum) without a stored result.
    pub closed_without_result: i64,
}

impl VotingCheck {
    pub fn healthy(&self) -> bool {
        self.duplicate_ballots == 0
            && self.ballots_outside_snapshot == 0
            && self.results_mismatch == 0
            && self.closed_without_result == 0
    }
}

#[tracing::instrument(level = "debug", skip_all, err(level = "debug"))]
pub async fn integrity(conn: &Connection) -> Result<VotingCheck, turso::Error> {
    let one = |sql: &'static str| async move {
        query_opt(conn, sql, ())
            .await?
            .map(|r| r.get::<i64>(0))
            .transpose()
            .map(|v| v.unwrap_or(0))
    };
    Ok(VotingCheck {
        ballots: one("SELECT COUNT(*) FROM ballots").await?,
        max_ballots_per_proposal: one(
            "SELECT COALESCE(MAX(n), 0) FROM (SELECT COUNT(*) AS n FROM ballots GROUP BY proposal_id)",
        )
        .await?,
        duplicate_ballots: one(
            "SELECT COUNT(*) FROM (SELECT proposal_id, user_id FROM ballots GROUP BY proposal_id, user_id HAVING COUNT(*) > 1)",
        )
        .await?,
        ballots_outside_snapshot: one(
            "SELECT COUNT(*) FROM ballots b JOIN proposals p ON p.id = b.proposal_id
             WHERE p.vote_snapshot_id IS NULL OR b.snapshot_id <> p.vote_snapshot_id
                OR NOT EXISTS (SELECT 1 FROM electorate_members m WHERE m.snapshot_id = b.snapshot_id AND m.user_id = b.user_id)",
        )
        .await?,
        results: one("SELECT COUNT(*) FROM vote_results").await?,
        results_mismatch: one(
            "SELECT COUNT(*) FROM vote_results r WHERE
               r.yes_count <> (SELECT COUNT(*) FROM ballots b WHERE b.proposal_id = r.proposal_id AND b.choice = 'yes')
               OR r.no_count <> (SELECT COUNT(*) FROM ballots b WHERE b.proposal_id = r.proposal_id AND b.choice = 'no')
               OR r.abstain_count <> (SELECT COUNT(*) FROM ballots b WHERE b.proposal_id = r.proposal_id AND b.choice = 'abstain')",
        )
        .await?,
        closed_without_result: one(
            "SELECT COUNT(*) FROM proposals p WHERE p.status IN ('approved', 'rejected', 'quorum_not_reached')
               AND NOT EXISTS (SELECT 1 FROM vote_results r WHERE r.proposal_id = p.id)",
        )
        .await?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outcome_storage_round_trips() {
        for o in [
            Outcome::Approved,
            Outcome::QuorumNotReached,
            Outcome::Rejected(RejectReason::NoValidVotes),
            Outcome::Rejected(RejectReason::Tie),
            Outcome::Rejected(RejectReason::MajorityNotReached),
        ] {
            let (a, b) = outcome_db(o);
            assert_eq!(outcome_from_db(a, b), Some(o));
        }
        assert_eq!(outcome_from_db("approved", Some("tie")), None);
    }
}
