//! Signature collection: one active signature per account and proposal, withdrawal
//! and re-signature only during collection, threshold applied in the same
//! transaction (a single serialized writer prevents double transitions).

use serde_json::json;
use turso::Connection;

use super::{
    actor::Actor,
    audit::{self, Event, Visibility},
    idempotency::{self, Key},
    phases,
    proposals::{self, Ctx, ProposalRow},
    reauth,
};
use crate::{
    domain::{
        identity::Permission,
        proposals::{ProposalStatus, view::SignatureState},
    },
    error::AppError,
    infrastructure::{clock::ts, db::query_opt, flags},
};

pub use crate::domain::proposals::REAUTH_SIGN;

/// Loads the proposal and checks it is in an open collection at `now`.
async fn open_collection(
    ctx: &Ctx<'_>,
    tx: &Connection,
    id: &str,
) -> Result<ProposalRow, AppError> {
    let p = proposals::load(tx, id).await?.ok_or(AppError::NotFound)?;
    if p.status == ProposalStatus::Draft {
        return Err(AppError::NotFound);
    }
    let open = p.status == ProposalStatus::CollectingSignatures
        && p.collect_ends_at
            .as_deref()
            .is_some_and(|d| ts(ctx.now).as_str() < d);
    if !open {
        return Err(AppError::PhaseClosed);
    }
    Ok(p)
}

async fn recount(tx: &Connection, ctx: &Ctx<'_>, id: &str) -> Result<u64, AppError> {
    let count: u64 = query_opt(
        tx,
        "SELECT COUNT(*) FROM signatures WHERE proposal_id = ?1 AND active = 1",
        [id],
    )
    .await?
    .ok_or(AppError::Internal)?
    .get(0)?;
    tx.execute(
        "UPDATE proposals SET signature_count = ?2, updated_at = ?3 WHERE id = ?1 AND status = 'collecting_signatures'",
        (id, count, ts(ctx.now)),
    )
    .await?;
    Ok(count)
}

fn key<'a>(
    actor: &'a Actor,
    raw: Option<&'a str>,
    action: &'a str,
    id: &str,
) -> Result<Option<Key<'a>>, AppError> {
    Ok(idempotency::parse(raw)?.map(|key| Key {
        user_id: &actor.user_id,
        key,
        action,
        request_hash: idempotency::request_hash(&[id]),
    }))
}

/// Signs during collection. Requires the `signatures` flag, eligibility in the
/// collection snapshot and a `proposal:sign` reauth grant consumed in the same
/// transaction. Reaching the target closes collection and opens the next phase.
pub async fn sign(
    ctx: &Ctx<'_>,
    actor: &Actor,
    id: &str,
    idempotency_key: Option<&str>,
) -> Result<SignatureState, AppError> {
    ctx.flags.require(flags::SIGNATURES)?;
    actor.require(Permission::ProposalSign)?;
    let key = key(actor, idempotency_key, "proposal.sign", id)?;
    let (tx, ctx) = ctx.write().await?;
    if let Some(key) = &key
        && let Some(previous) = idempotency::lookup(&tx, key).await?
    {
        return Ok(previous);
    }
    let mut p = open_collection(&ctx, &tx, id).await?;
    let snapshot = p
        .collection_snapshot_id
        .as_deref()
        .ok_or(AppError::Internal)?;
    if !actor.participates_in(&p.jurisdiction_id)
        || !proposals::in_snapshot(&tx, snapshot, &actor.user_id).await?
    {
        return Err(AppError::Forbidden);
    }
    let active: Option<i64> = query_opt(
        &tx,
        "SELECT active FROM signatures WHERE proposal_id = ?1 AND user_id = ?2",
        (id, actor.user_id.as_str()),
    )
    .await?
    .map(|r| r.get(0))
    .transpose()?;
    if active == Some(1) {
        return Err(AppError::Conflict(
            crate::i18n::t("proposals.error.already_signed").into(),
        ));
    }
    reauth::consume(&tx, ctx.now, &actor.session_hash, REAUTH_SIGN, id).await?;
    let text_hash = p.text_hash.clone().ok_or(AppError::Internal)?;
    tx.execute(
        "INSERT INTO signatures (proposal_id, user_id, text_hash, active, signed_at, updated_at) VALUES (?1, ?2, ?3, 1, ?4, ?4)
         ON CONFLICT(proposal_id, user_id) DO UPDATE SET active = 1, signed_at = excluded.signed_at, updated_at = excluded.updated_at",
        (id, actor.user_id.as_str(), text_hash, ts(ctx.now)),
    )
    .await?;
    let count = recount(&tx, &ctx, id).await?;
    let target = p.signature_target.ok_or(AppError::Internal)?;
    audit::append(
        &tx,
        ctx.now,
        Event {
            kind: "proposal.signature_added",
            visibility: Visibility::Public,
            actor_ref: None,
            subject: Some(id),
            payload: json!({ "signature_count": count, "signature_target": target }),
        },
    )
    .await?;
    let mut status = p.status;
    if count >= target {
        p.signature_count = count;
        status = phases::close_collection(&tx, ctx.now, &p).await?;
    }
    let result = SignatureState {
        status,
        signature_count: count,
        signature_target: target,
        signed: true,
    };
    if let Some(key) = &key {
        idempotency::store(&tx, ctx.now, key, &result).await?;
    }
    tx.commit().await?;
    Ok(result)
}

/// Withdraws the caller's active signature while collection is open.
pub async fn withdraw_signature(
    ctx: &Ctx<'_>,
    actor: &Actor,
    id: &str,
    idempotency_key: Option<&str>,
) -> Result<SignatureState, AppError> {
    ctx.flags.require(flags::SIGNATURES)?;
    actor.require(Permission::ProposalSign)?;
    let key = key(actor, idempotency_key, "proposal.unsign", id)?;
    let (tx, ctx) = ctx.write().await?;
    if let Some(key) = &key
        && let Some(previous) = idempotency::lookup(&tx, key).await?
    {
        return Ok(previous);
    }
    let p = open_collection(&ctx, &tx, id).await?;
    let changed = tx
        .execute(
            "UPDATE signatures SET active = 0, updated_at = ?3 WHERE proposal_id = ?1 AND user_id = ?2 AND active = 1",
            (id, actor.user_id.as_str(), ts(ctx.now)),
        )
        .await?;
    if changed != 1 {
        return Err(AppError::Conflict(
            crate::i18n::t("proposals.error.not_signed").into(),
        ));
    }
    let count = recount(&tx, &ctx, id).await?;
    let target = p.signature_target.ok_or(AppError::Internal)?;
    audit::append(
        &tx,
        ctx.now,
        Event {
            kind: "proposal.signature_withdrawn",
            visibility: Visibility::Public,
            actor_ref: None,
            subject: Some(id),
            payload: json!({ "signature_count": count, "signature_target": target }),
        },
    )
    .await?;
    let result = SignatureState {
        status: p.status,
        signature_count: count,
        signature_target: target,
        signed: false,
    };
    if let Some(key) = &key {
        idempotency::store(&tx, ctx.now, key, &result).await?;
    }
    tx.commit().await?;
    Ok(result)
}
