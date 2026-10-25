//! Territorial filters on accounts. `users` stores the municipality, state and
//! country ids; the column is chosen from the jurisdiction kind so the lookup
//! uses an index (`municipality_id = ?1 OR state_id = ?1 OR country_id = ?1`
//! scans every account).

use turso::Connection;

use crate::{domain::proposals::Level, error::AppError, infrastructure::db::query_opt};

/// `users` column holding a jurisdiction of this kind; `None` when unknown.
pub async fn users_column(
    conn: &Connection,
    jurisdiction_id: &str,
) -> Result<Option<&'static str>, AppError> {
    let Some(row) = query_opt(
        conn,
        "SELECT kind FROM jurisdictions WHERE id = ?1",
        [jurisdiction_id],
    )
    .await?
    else {
        return Ok(None);
    };
    Ok(
        Level::from_db(&row.get::<String>(0)?).map(|level| match level {
            Level::Municipal => "municipality_id",
            Level::State => "state_id",
            Level::National => "country_id",
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::{
        catalog,
        db::{query_all, testing::migrated},
    };

    /// Every territorial account filter has an index for its column.
    #[tokio::test]
    async fn chosen_columns_use_an_index() {
        let t = migrated().await;
        catalog::import_str(
            &t.db,
            "mini",
            "code,kind,name,parent_code,state_abbrev\nBR,national,Brasil,,\n42,state,SC,BR,SC\n4202404,municipal,Blumenau,42,SC\n",
        )
        .await
        .unwrap();
        let conn = t.db.read().await.unwrap();
        for (code, expected) in [
            ("4202404", "municipality_id"),
            ("42", "state_id"),
            ("BR", "country_id"),
        ] {
            let id: String = query_opt(
                &conn,
                "SELECT id FROM jurisdictions WHERE code = ?1",
                [code],
            )
            .await
            .unwrap()
            .unwrap()
            .get(0)
            .unwrap();
            let column = users_column(&conn, &id).await.unwrap();
            assert_eq!(column, Some(expected));
            let plan: Vec<String> = query_all(
                &conn,
                &format!("EXPLAIN QUERY PLAN SELECT id FROM users WHERE status = 'active' AND {expected} = ?1"),
                [id.as_str()],
            )
            .await
            .unwrap()
            .iter()
            .map(|r| r.get::<String>(3).unwrap())
            .collect();
            assert!(
                plan.iter()
                    .any(|p| p.contains("USING INDEX") || p.contains("USING COVERING INDEX")),
                "{expected}: {plan:?}"
            );
        }
        assert_eq!(users_column(&conn, "unknown").await.unwrap(), None);
    }
}
