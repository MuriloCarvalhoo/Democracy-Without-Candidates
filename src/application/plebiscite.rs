//! Initial setup plebiscite and meta-law parameter views. The country admin only
//! starts the plebiscite with the suggested package (never edits or adopts it);
//! adoption happens when the vote closes approved (`phases::close_vote`).

use chrono::{DateTime, Utc};
use turso::Connection;

use super::{actor::Actor, phases, proposals, setup};
use crate::{
    domain::{
        identity::Permission,
        proposals::{ProposalKind, ProposalStatus},
        setup::SUGGESTED,
        voting::view::{SetupDiff, SetupOverview},
    },
    error::AppError,
    i18n::t,
    infrastructure::{
        db::Db,
        flags::{self, Flags},
    },
};

#[tracing::instrument(level = "debug", skip_all, fields(viewer = ?viewer.map(|a| a.user_id.as_str())), err(level = "debug"))]
pub async fn overview(
    conn: &Connection,
    tz: chrono_tz::Tz,
    country_id: &str,
    viewer: Option<&Actor>,
) -> Result<SetupOverview, AppError> {
    let effective = setup::effective_setup(conn, country_id).await?;
    let mut open = proposals::summaries(
        conn,
        tz,
        "WHERE p.kind = 'initial_setup' AND p.status = 'in_voting' ORDER BY p.published_at DESC LIMIT 1",
        Vec::new(),
    )
    .await?;
    let past = proposals::summaries(
        conn,
        tz,
        "WHERE p.kind = 'initial_setup' AND p.status <> 'in_voting' AND p.status <> 'draft' ORDER BY p.published_at DESC LIMIT 20",
        Vec::new(),
    )
    .await?;
    let open_plebiscite = open.pop();
    let can_start = viewer.is_some_and(|a| a.can(Permission::SetupStart))
        && effective.is_none()
        && open_plebiscite.is_none();
    Ok(SetupOverview {
        effective_version: effective.as_ref().and_then(|s| s.version),
        effective_adopted_at: effective
            .and_then(|s| s.adopted_at)
            .map(|at| proposals::local(&at, tz)),
        open_plebiscite,
        past_plebiscites: past,
        suggested: SUGGESTED,
        can_start,
    })
}

/// Starts the national plebiscite with the suggested package. Only one may be
/// open; a new one only after rejection or quorum failure (no adopted setup).
#[tracing::instrument(level = "debug", skip_all, fields(actor = %actor.user_id), err(level = "debug"))]
pub async fn start(
    db: &Db,
    flags: &Flags,
    now: DateTime<Utc>,
    actor: &Actor,
) -> Result<String, AppError> {
    actor.require(Permission::SetupStart)?;
    flags.require(flags::VOTING)?;
    phases::start_initial_setup(
        db,
        now,
        actor,
        &SUGGESTED,
        t("voting.setup.plebiscite_title"),
        t("voting.setup.plebiscite_body"),
    )
    .await
}

/// Current effective parameters of the proposal's jurisdiction vs the package
/// proposed by a meta-law (or plebiscite).
#[tracing::instrument(level = "debug", skip_all, fields(id = %id), err(level = "debug"))]
pub async fn diff(conn: &Connection, id: &str) -> Result<SetupDiff, AppError> {
    let p = proposals::load(conn, id).await?.ok_or(AppError::NotFound)?;
    if p.status == ProposalStatus::Draft
        || !matches!(
            p.kind,
            ProposalKind::SetupChange | ProposalKind::InitialSetup
        )
    {
        return Err(AppError::NotFound);
    }
    let proposed = setup::get(
        conn,
        p.proposed_setup_id
            .as_deref()
            .ok_or_else(|| AppError::internal("missing or invalid stored value"))?,
    )
    .await?
    .ok_or_else(|| AppError::internal("missing or invalid stored value"))?;
    // Once approved, the package itself is effective: no "current" to compare.
    let current = setup::effective_setup(conn, &p.jurisdiction_id)
        .await?
        .filter(|s| s.id != proposed.id);
    Ok(SetupDiff {
        jurisdiction: proposals::jurisdiction_label(conn, &p.jurisdiction_id).await?,
        current_version: current.as_ref().and_then(|s| s.version),
        current: current.map(|s| s.params),
        proposed: proposed.params,
    })
}
