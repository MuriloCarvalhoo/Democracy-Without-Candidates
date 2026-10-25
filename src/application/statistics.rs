//! Public aggregate statistics. Only counts are published; territorial counts of
//! accounts below `MIN_CELL` are suppressed (see `domain::voting::suppress`).

use turso::Connection;

use crate::{
    domain::{
        proposals::{ProposalKind, ProposalStatus},
        voting::{
            MIN_CELL, suppress,
            view::{ClosedVoteStat, Statistics, TerritoryCount},
        },
    },
    error::AppError,
    infrastructure::db::query_all,
};

pub async fn statistics(conn: &Connection) -> Result<Statistics, AppError> {
    let mut by_status = Vec::new();
    for row in query_all(
        conn,
        "SELECT status, COUNT(*) FROM proposals WHERE status <> 'draft' GROUP BY status",
        (),
    )
    .await?
    {
        let status = ProposalStatus::from_db(&row.get::<String>(0)?).ok_or(AppError::Internal)?;
        by_status.push((status, row.get::<u64>(1)?));
    }
    by_status.sort_by_key(|(s, _)| ProposalStatus::ALL.iter().position(|x| x == s));
    let mut by_kind = Vec::new();
    for row in query_all(
        conn,
        "SELECT kind, COUNT(*) FROM proposals WHERE status <> 'draft' GROUP BY kind",
        (),
    )
    .await?
    {
        let kind = ProposalKind::from_db(&row.get::<String>(0)?).ok_or(AppError::Internal)?;
        by_kind.push((kind, row.get::<u64>(1)?));
    }
    by_kind.sort_by_key(|(k, _)| ProposalKind::ALL.iter().position(|x| x == k));
    let mut closed_votes = Vec::new();
    for row in query_all(
        conn,
        "SELECT p.id, p.title, p.status, r.electorate, r.yes_count + r.no_count + r.abstain_count
         FROM vote_results r JOIN proposals p ON p.id = r.proposal_id
         ORDER BY r.closed_at DESC LIMIT 50",
        (),
    )
    .await?
    {
        closed_votes.push(ClosedVoteStat {
            id: row.get(0)?,
            title: row.get(1)?,
            status: ProposalStatus::from_db(&row.get::<String>(2)?).ok_or(AppError::Internal)?,
            electorate: row.get(3)?,
            participation: row.get(4)?,
        });
    }
    let rows = query_all(
        conn,
        "SELECT j.name, COUNT(*) FROM users u JOIN jurisdictions j ON j.id = u.state_id
         WHERE u.status = 'active' AND EXISTS (SELECT 1 FROM user_roles r WHERE r.user_id = u.id AND r.role = 'CITIZEN')
         GROUP BY j.id, j.name ORDER BY j.name",
        (),
    )
    .await?;
    let mut labels = Vec::with_capacity(rows.len());
    let mut counts = Vec::with_capacity(rows.len());
    for row in rows {
        labels.push(row.get::<String>(0)?);
        counts.push(row.get::<u64>(1)?);
    }
    let (cells, citizens_total) = suppress(&counts);
    Ok(Statistics {
        by_status,
        by_kind,
        closed_votes,
        citizens_by_state: labels
            .into_iter()
            .zip(cells)
            .map(|(label, count)| TerritoryCount { label, count })
            .collect(),
        citizens_total,
        min_cell: MIN_CELL,
    })
}
