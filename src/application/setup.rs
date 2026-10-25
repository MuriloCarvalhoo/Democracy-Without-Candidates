//! Setup versions per jurisdiction. Adoption only happens through an approved
//! plebiscite/meta-law (voting feature calls `adopt_proposed`) or, outside
//! production, through synthetic fixtures. Never edited in place.

use chrono::{DateTime, Utc};
use serde_json::json;
use turso::{Connection, Row, Value};

use super::audit::{self, Event, Visibility};
use crate::{
    domain::{proposals::view::SetupVersionView, setup::SetupParams},
    error::AppError,
    infrastructure::{
        clock::ts,
        crypto::new_id,
        db::{query_all, query_opt},
    },
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetupVersion {
    pub id: String,
    pub jurisdiction_id: String,
    pub version: Option<u32>,
    pub status: String,
    pub synthetic: bool,
    pub source_proposal_id: Option<String>,
    pub adopted_at: Option<String>,
    pub params: SetupParams,
}

fn param_columns() -> String {
    crate::domain::setup::SUGGESTED
        .fields()
        .map(|(name, _)| name)
        .join(", ")
}

fn select_sql(filter: &str) -> String {
    format!(
        "SELECT id, jurisdiction_id, version, status, synthetic, source_proposal_id, adopted_at, {} FROM setup_versions {filter}",
        param_columns()
    )
}

fn from_row(row: &Row) -> Result<SetupVersion, AppError> {
    let mut values = [0u32; 22];
    for (i, v) in values.iter_mut().enumerate() {
        *v = row.get(7 + i)?;
    }
    Ok(SetupVersion {
        id: row.get(0)?,
        jurisdiction_id: row.get(1)?,
        version: row.get::<Option<u32>>(2)?,
        status: row.get(3)?,
        synthetic: row.get::<i64>(4)? == 1,
        source_proposal_id: row.get(5)?,
        adopted_at: row.get(6)?,
        params: SetupParams::from_values(values),
    })
}

#[tracing::instrument(level = "debug", skip_all, fields(id = %id), err(level = "debug"))]
pub async fn get(conn: &Connection, id: &str) -> Result<Option<SetupVersion>, AppError> {
    match query_opt(conn, &select_sql("WHERE id = ?1"), [id]).await? {
        Some(row) => Ok(Some(from_row(&row)?)),
        None => Ok(None),
    }
}

/// Effective setup for a jurisdiction: its own adopted version, otherwise the
/// nearest adopted ancestor (municipal → state → national).
#[tracing::instrument(level = "debug", skip_all, fields(jurisdiction_id = %jurisdiction_id), err(level = "debug"))]
pub async fn effective_setup(
    conn: &Connection,
    jurisdiction_id: &str,
) -> Result<Option<SetupVersion>, AppError> {
    let mut current = Some(jurisdiction_id.to_owned());
    while let Some(j) = current {
        if let Some(row) = query_opt(
            conn,
            &select_sql(
                "WHERE jurisdiction_id = ?1 AND status = 'adopted' ORDER BY version DESC LIMIT 1",
            ),
            [j.as_str()],
        )
        .await?
        {
            return Ok(Some(from_row(&row)?));
        }
        current = query_opt(
            conn,
            "SELECT parent_id FROM jurisdictions WHERE id = ?1",
            [j.as_str()],
        )
        .await?
        .map(|r| r.get::<Option<String>>(0))
        .transpose()?
        .flatten();
    }
    Ok(None)
}

/// Inserts a `proposed` package (voted by a setup_change/initial_setup proposal).
#[tracing::instrument(level = "debug", skip_all, fields(jurisdiction_id = %jurisdiction_id, synthetic = %synthetic), err(level = "debug"))]
pub async fn insert_proposed(
    conn: &Connection,
    now: DateTime<Utc>,
    jurisdiction_id: &str,
    params: &SetupParams,
    synthetic: bool,
) -> Result<String, AppError> {
    params.validate().map_err(|field| {
        AppError::validation(field, crate::i18n::t("proposals.error.setup_param"))
    })?;
    let id = new_id();
    let fields = params.fields();
    let placeholders: Vec<String> = (6..6 + fields.len()).map(|i| format!("?{i}")).collect();
    let sql = format!(
        "INSERT INTO setup_versions (id, jurisdiction_id, status, synthetic, created_at, {}) VALUES (?1, ?2, ?3, ?4, ?5, {})",
        param_columns(),
        placeholders.join(", ")
    );
    let mut values: Vec<Value> = vec![
        id.clone().into(),
        jurisdiction_id.into(),
        "proposed".into(),
        i64::from(synthetic).into(),
        ts(now).into(),
    ];
    values.extend(fields.iter().map(|(_, v)| Value::from(*v)));
    conn.execute(&sql, values).await?;
    Ok(id)
}

/// Adopts a proposed package inside the caller's transaction: assigns the next
/// version, supersedes the previous adopted version of the same jurisdiction and
/// records a public audit event. Applies only to elections opened afterwards
/// (proposals keep the version ids captured at each phase opening).
/// Called by the voting feature when a setup plebiscite/meta-law is approved.
#[tracing::instrument(level = "debug", skip_all, fields(setup_version_id = %setup_version_id, source_proposal_id = ?source_proposal_id), err(level = "debug"))]
pub async fn adopt_proposed(
    conn: &Connection,
    now: DateTime<Utc>,
    setup_version_id: &str,
    source_proposal_id: Option<&str>,
) -> Result<u32, AppError> {
    let setup = get(conn, setup_version_id)
        .await?
        .ok_or(AppError::NotFound)?;
    if setup.status != "proposed" {
        return Err(AppError::Conflict("setup não está proposto".into()));
    }
    let next: u32 = query_opt(
        conn,
        "SELECT COALESCE(MAX(version), 0) + 1 FROM setup_versions WHERE jurisdiction_id = ?1",
        [setup.jurisdiction_id.as_str()],
    )
    .await?
    .ok_or_else(|| AppError::internal("missing or invalid stored value"))?
    .get(0)?;
    conn.execute(
        "UPDATE setup_versions SET status = 'superseded' WHERE jurisdiction_id = ?1 AND status = 'adopted'",
        [setup.jurisdiction_id.as_str()],
    )
    .await?;
    conn.execute(
        "UPDATE setup_versions SET status = 'adopted', version = ?2, adopted_at = ?3, source_proposal_id = ?4 WHERE id = ?1",
        (setup_version_id, next, ts(now), source_proposal_id),
    )
    .await?;
    audit::append(
        conn,
        now,
        Event {
            kind: "setup.adopted",
            visibility: Visibility::Public,
            actor_ref: None,
            subject: Some(setup_version_id),
            payload: json!({
                "jurisdiction_id": setup.jurisdiction_id,
                "version": next,
                "source_proposal_id": source_proposal_id,
                "synthetic": setup.synthetic,
            }),
        },
    )
    .await?;
    Ok(next)
}

/// Marks a proposed package rejected (plebiscite rejected or quorum not reached).
#[tracing::instrument(level = "debug", skip_all, fields(setup_version_id = %setup_version_id), err(level = "debug"))]
pub async fn reject_proposed(conn: &Connection, setup_version_id: &str) -> Result<(), AppError> {
    let changed = conn
        .execute(
            "UPDATE setup_versions SET status = 'rejected' WHERE id = ?1 AND status = 'proposed'",
            [setup_version_id],
        )
        .await?;
    if changed == 1 {
        Ok(())
    } else {
        Err(AppError::Conflict("setup não está proposto".into()))
    }
}

/// Direct adoption, inside the caller's transaction. Only for synthetic
/// fixtures/tests or the approved-plebiscite path; never exposed to clients.
#[tracing::instrument(level = "debug", skip_all, fields(jurisdiction_id = %jurisdiction_id, source_proposal_id = ?source_proposal_id, synthetic = %synthetic), err(level = "debug"))]
pub async fn adopt_setup(
    conn: &Connection,
    now: DateTime<Utc>,
    jurisdiction_id: &str,
    params: &SetupParams,
    source_proposal_id: Option<&str>,
    synthetic: bool,
) -> Result<String, AppError> {
    let id = insert_proposed(conn, now, jurisdiction_id, params, synthetic).await?;
    adopt_proposed(conn, now, &id, source_proposal_id).await?;
    Ok(id)
}

/// Public history of every setup version, newest first.
#[tracing::instrument(level = "debug", skip_all, err(level = "debug"))]
pub async fn history(conn: &Connection) -> Result<Vec<SetupVersionView>, AppError> {
    let rows = query_all(
        conn,
        &select_sql(
            "WHERE status IN ('adopted', 'superseded') ORDER BY adopted_at DESC, version DESC LIMIT 200",
        ),
        (),
    )
    .await?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let v = from_row(&row)?;
        out.push(SetupVersionView {
            jurisdiction: super::proposals::jurisdiction_label(conn, &v.jurisdiction_id).await?,
            id: v.id,
            version: v.version,
            status: v.status,
            synthetic: v.synthetic,
            adopted_at: v.adopted_at,
            source_proposal_id: v.source_proposal_id,
            params: v.params,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        domain::setup::SUGGESTED,
        infrastructure::{catalog, db::testing::migrated},
    };

    pub const MINI: &str = "code,kind,name,parent_code,state_abbrev\nBR,national,Brasil,,\n42,state,Santa Catarina,BR,SC\n4202404,municipal,Blumenau,42,SC\n";

    async fn id_of(conn: &Connection, code: &str) -> String {
        query_opt(conn, "SELECT id FROM jurisdictions WHERE code = ?1", [code])
            .await
            .unwrap()
            .unwrap()
            .get(0)
            .unwrap()
    }

    #[tokio::test]
    async fn national_setup_is_inherited_until_local_adoption() {
        let t = migrated().await;
        catalog::import_str(&t.db, "mini", MINI).await.unwrap();
        let now = Utc::now();
        let tx = t.db.write().await.unwrap();
        let (br, sc, bnu) = (
            id_of(&tx, "BR").await,
            id_of(&tx, "42").await,
            id_of(&tx, "4202404").await,
        );
        assert_eq!(effective_setup(&tx, &bnu).await.unwrap(), None);
        let national = adopt_setup(&tx, now, &br, &SUGGESTED, None, true)
            .await
            .unwrap();
        assert_eq!(
            effective_setup(&tx, &bnu).await.unwrap().unwrap().id,
            national
        );
        let mut local = SUGGESTED;
        local.signature_bp_state = 150;
        let state = adopt_setup(&tx, now, &sc, &local, None, true)
            .await
            .unwrap();
        let eff = effective_setup(&tx, &bnu).await.unwrap().unwrap();
        assert_eq!((eff.id.as_str(), eff.version), (state.as_str(), Some(1)));
        assert_eq!(
            effective_setup(&tx, &br).await.unwrap().unwrap().id,
            national
        );

        // A second national adoption supersedes the first and gets version 2.
        let v2 = adopt_setup(&tx, now, &br, &SUGGESTED, None, true)
            .await
            .unwrap();
        let eff = effective_setup(&tx, &br).await.unwrap().unwrap();
        assert_eq!((eff.id, eff.version), (v2, Some(2)));
        assert_eq!(
            get(&tx, &national).await.unwrap().unwrap().status,
            "superseded"
        );
        tx.commit().await.unwrap();
    }

    #[tokio::test]
    async fn parameters_and_history_are_immutable_in_the_database() {
        let t = migrated().await;
        catalog::import_str(&t.db, "mini", MINI).await.unwrap();
        let tx = t.db.write().await.unwrap();
        let br = id_of(&tx, "BR").await;
        let id = adopt_setup(&tx, Utc::now(), &br, &SUGGESTED, None, true)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        let conn = t.db.read().await.unwrap();
        assert!(
            conn.execute(
                "UPDATE setup_versions SET normal_quorum_bp = 1 WHERE id = ?1",
                [id.as_str()]
            )
            .await
            .is_err()
        );
        assert!(
            conn.execute("DELETE FROM setup_versions WHERE id = ?1", [id.as_str()])
                .await
                .is_err()
        );
        assert!(
            conn.execute(
                "INSERT INTO setup_versions (id, jurisdiction_id, status, created_at, signature_bp_national, signature_bp_state, signature_bp_municipal, normal_quorum_bp, normal_majority_bp, normal_collect_days, normal_debate_days, normal_vote_days, urgent_quorum_bp, urgent_majority_bp, urgent_collect_days, urgent_vote_days, setup_quorum_bp, setup_majority_bp, setup_collect_days, setup_debate_days, setup_vote_days, initial_quorum_bp, initial_majority_bp, initial_vote_days, quota_normal_per_year, quota_urgent_per_year)
                 VALUES ('x', ?1, 'proposed', 'now', 50, 100, 200, 2500, 4000, 90, 15, 30, 2000, 5000, 15, 7, 3500, 6000, 90, 15, 30, 4000, 6000, 30, 5, 1)",
                [br.as_str()],
            )
            .await
            .is_err(),
            "majority below 50% rejected by CHECK"
        );
        let tx = t.db.write().await.unwrap();
        assert!(
            adopt_proposed(&tx, Utc::now(), &id, None).await.is_err(),
            "already adopted"
        );
        tx.rollback().await.unwrap();
        assert_eq!(history(&conn).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn rejected_package_does_not_become_effective() {
        let t = migrated().await;
        catalog::import_str(&t.db, "mini", MINI).await.unwrap();
        let tx = t.db.write().await.unwrap();
        let br = id_of(&tx, "BR").await;
        let id = insert_proposed(&tx, Utc::now(), &br, &SUGGESTED, true)
            .await
            .unwrap();
        reject_proposed(&tx, &id).await.unwrap();
        assert!(reject_proposed(&tx, &id).await.is_err());
        assert_eq!(effective_setup(&tx, &br).await.unwrap(), None);
        tx.commit().await.unwrap();
    }
}
