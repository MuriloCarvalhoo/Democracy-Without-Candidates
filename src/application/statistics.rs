//! Public aggregate statistics. Only counts are published; territorial counts of
//! accounts below `MIN_CELL` are suppressed (see `domain::voting::suppress`).

use std::{
    sync::{LazyLock, Mutex},
    time::{Duration, Instant},
};

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

/// How long the citizens-by-state counts (a scan of every account) are reused.
pub const CITIZEN_COUNTS_TTL: Duration = Duration::from_secs(60);

type StateCounts = Vec<(String, u64)>;

/// Unsuppressed citizens-by-state counts of the last scan; suppression is
/// applied on every read, so the published cells never change rule.
#[derive(Default)]
pub struct CitizenCounts(Mutex<Option<(Instant, StateCounts)>>);

/// The cache used by the public statistics page (one process per database).
pub fn process_citizen_counts() -> &'static CitizenCounts {
    static CACHE: LazyLock<CitizenCounts> = LazyLock::new(Default::default);
    &CACHE
}

async fn citizens_by_state(
    conn: &Connection,
    cache: &CitizenCounts,
    now: Instant,
) -> Result<StateCounts, AppError> {
    if let Ok(cached) = cache.0.lock()
        && let Some((at, counts)) = cached.as_ref()
        && now.saturating_duration_since(*at) < CITIZEN_COUNTS_TTL
    {
        return Ok(counts.clone());
    }
    let rows = query_all(
        conn,
        "SELECT j.name, COUNT(*) FROM users u JOIN jurisdictions j ON j.id = u.state_id
         WHERE u.status = 'active' AND (EXISTS (SELECT 1 FROM iam_user_policies up WHERE up.user_id = u.id AND up.policy_id = 'pol-cidadao')
           OR EXISTS (SELECT 1 FROM iam_user_groups ug JOIN iam_group_policies gp ON gp.group_id = ug.group_id
                      WHERE ug.user_id = u.id AND gp.policy_id = 'pol-cidadao'))
         GROUP BY j.id, j.name ORDER BY j.name",
        (),
    )
    .await?;
    let mut counts = Vec::with_capacity(rows.len());
    for row in rows {
        counts.push((row.get::<String>(0)?, row.get::<u64>(1)?));
    }
    if let Ok(mut cached) = cache.0.lock() {
        *cached = Some((now, counts.clone()));
    }
    Ok(counts)
}

#[tracing::instrument(level = "debug", skip_all, err(level = "debug"))]
pub async fn statistics(conn: &Connection, cache: &CitizenCounts) -> Result<Statistics, AppError> {
    statistics_at(conn, cache, Instant::now()).await
}

async fn statistics_at(
    conn: &Connection,
    cache: &CitizenCounts,
    now: Instant,
) -> Result<Statistics, AppError> {
    let mut by_status = Vec::new();
    for row in query_all(
        conn,
        "SELECT status, COUNT(*) FROM proposals WHERE status <> 'draft' GROUP BY status",
        (),
    )
    .await?
    {
        let status = ProposalStatus::from_db(&row.get::<String>(0)?)
            .ok_or_else(|| AppError::internal("missing or invalid stored value"))?;
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
        let kind = ProposalKind::from_db(&row.get::<String>(0)?)
            .ok_or_else(|| AppError::internal("missing or invalid stored value"))?;
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
            status: ProposalStatus::from_db(&row.get::<String>(2)?)
                .ok_or_else(|| AppError::internal("missing or invalid stored value"))?,
            electorate: row.get(3)?,
            participation: row.get(4)?,
        });
    }
    let (labels, counts): (Vec<String>, Vec<u64>) = citizens_by_state(conn, cache, now)
        .await?
        .into_iter()
        .unzip();
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        application::session::{SessionPolicy, testing::citizen_with_session},
        infrastructure::{catalog, db::testing::migrated},
    };

    /// Counts are reused within the TTL and refreshed after it; small cells stay
    /// suppressed whether the counts come from the cache or a new scan.
    #[tokio::test]
    async fn citizen_counts_are_cached_briefly_and_still_suppressed() {
        let t = migrated().await;
        catalog::import_str(&t.db, "mini", "code,kind,name,parent_code,state_abbrev\nBR,national,Brasil,,\n42,state,SC,BR,SC\n4202404,municipal,Blumenau,42,SC\n")
            .await
            .unwrap();
        let now = chrono::Utc::now();
        let policy = SessionPolicy::from_secs(900, 28800);
        citizen_with_session(&t.db, now, policy, "4202404").await;
        let cache = CitizenCounts::default();
        let conn = t.db.read().await.unwrap();
        let start = Instant::now();
        let first = statistics_at(&conn, &cache, start).await.unwrap();
        assert_eq!(first.citizens_by_state.len(), 1);
        assert_eq!(first.citizens_by_state[0].count, None, "below MIN_CELL");
        for _ in 0..MIN_CELL {
            citizen_with_session(&t.db, now, policy, "4202404").await;
        }
        let cached = statistics_at(&conn, &cache, start + Duration::from_secs(59))
            .await
            .unwrap();
        assert_eq!(cached.citizens_by_state, first.citizens_by_state);
        assert_eq!(cached.citizens_total, first.citizens_total);
        let fresh = statistics_at(&conn, &cache, start + CITIZEN_COUNTS_TTL)
            .await
            .unwrap();
        assert_eq!(fresh.citizens_by_state[0].count, Some(MIN_CELL + 1));
    }
}
