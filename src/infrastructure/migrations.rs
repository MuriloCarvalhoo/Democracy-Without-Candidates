//! Versioned SQL migrations with checksums. Applied in order, each in its own
//! transaction; an edited applied migration aborts startup instead of diverging.

use super::{clock::ts, crypto::sha256_hex, db::Db};

pub struct Migration {
    pub version: &'static str,
    pub sql: &'static str,
}

macro_rules! migration {
    ($version:literal) => {
        Migration {
            version: $version,
            sql: include_str!(concat!("../../migrations/", $version, ".sql")),
        }
    };
}

/// Ordered list. Ranges per area: 0001-0099 foundation, 0100 identity,
/// 0200 setup/proposals, 0300 debate/notifications, 0400 voting/audit, 0500 admin.
pub const MIGRATIONS: &[Migration] = &[
    migration!("0001_foundation"),
    migration!("0002_identity_core"),
    migration!("0100_identity"),
    migration!("0200_setup_proposals"),
    migration!("0300_debate"),
    migration!("0400_voting"),
    migration!("0450_audit_notifications"),
    migration!("0500_iam"),
    migration!("0520_performance"),
];

#[derive(Debug, thiserror::Error)]
pub enum MigrationError {
    #[error("database: {0}")]
    Db(#[from] turso::Error),
    #[error("migration {0} was modified after being applied (checksum mismatch)")]
    Modified(String),
    #[error("database has unknown migration {0}; refusing to run an older binary")]
    Unknown(String),
}

pub async fn run(db: &Db) -> Result<Vec<&'static str>, MigrationError> {
    let tx = db.write().await?;
    tx.execute(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version TEXT PRIMARY KEY,
            checksum TEXT NOT NULL,
            applied_at TEXT NOT NULL
        )",
        (),
    )
    .await?;
    tx.commit().await?;

    let applied_versions = verified_applied(&*db.read().await?).await?;
    let mut newly = Vec::new();
    for m in MIGRATIONS {
        if applied_versions.iter().any(|v| v == m.version) {
            continue;
        }
        let tx = db.write().await?;
        tx.execute_batch(m.sql).await?;
        tx.execute(
            "INSERT INTO schema_migrations (version, checksum, applied_at) VALUES (?1, ?2, ?3)",
            (m.version, sha256_hex(m.sql), ts(chrono::Utc::now())),
        )
        .await?;
        tx.commit().await?;
        newly.push(m.version);
    }
    Ok(newly)
}

/// Applied versions, failing on edited or unknown migrations. Read-only.
async fn verified_applied(conn: &turso::Connection) -> Result<Vec<String>, MigrationError> {
    let applied = super::db::query_all(
        conn,
        "SELECT version, checksum FROM schema_migrations ORDER BY version",
        (),
    )
    .await?;
    let mut versions = Vec::new();
    for row in applied {
        let version: String = row.get(0)?;
        let checksum: String = row.get(1)?;
        match MIGRATIONS.iter().find(|m| m.version == version) {
            Some(m) if sha256_hex(m.sql) == checksum => versions.push(version),
            Some(_) => return Err(MigrationError::Modified(version)),
            None => return Err(MigrationError::Unknown(version)),
        }
    }
    Ok(versions)
}

/// Read-only check used on backups/restores: `(applied, pending)` for this binary.
pub async fn status(
    conn: &turso::Connection,
) -> Result<(Vec<String>, Vec<&'static str>), MigrationError> {
    let applied = verified_applied(conn).await?;
    let pending = MIGRATIONS
        .iter()
        .map(|m| m.version)
        .filter(|v| !applied.iter().any(|a| a == v))
        .collect();
    Ok((applied, pending))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::db::testing::migrated;

    #[tokio::test]
    async fn migrations_apply_once_and_are_idempotent() {
        let t = migrated().await;
        let again = run(&t.db).await.unwrap();
        assert!(again.is_empty());
    }

    #[tokio::test]
    async fn modified_migration_is_rejected() {
        let t = migrated().await;
        let tx = t.db.write().await.unwrap();
        tx.execute(
            "UPDATE schema_migrations SET checksum = 'x' WHERE version = '0001_foundation'",
            (),
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        assert!(matches!(run(&t.db).await, Err(MigrationError::Modified(_))));
    }

    /// Existing roles become policy attachments and `user_roles` is dropped.
    #[tokio::test]
    async fn iam_migration_converts_roles_into_attachments() {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(&dir.path().join("roles.db")).await.unwrap();
        let tx = db.write().await.unwrap();
        tx.execute(
            "CREATE TABLE schema_migrations (version TEXT PRIMARY KEY, checksum TEXT NOT NULL, applied_at TEXT NOT NULL)",
            (),
        )
        .await
        .unwrap();
        for m in MIGRATIONS.iter().take_while(|m| m.version != "0500_iam") {
            tx.execute_batch(m.sql).await.unwrap();
            tx.execute(
                "INSERT INTO schema_migrations VALUES (?1, ?2, 'before')",
                (m.version, sha256_hex(m.sql)),
            )
            .await
            .unwrap();
        }
        tx.execute_batch(
            "INSERT INTO jurisdictions (id, code, kind, name) VALUES ('br', 'BR', 'national', 'Brasil');
             INSERT INTO users (id, email, name, password_hash, status, municipality_id, state_id, country_id, created_at, updated_at) VALUES
               ('u-cit', 'c@x.test', 'C', 'x', 'active', 'br', 'br', 'br', 't', 't'),
               ('u-aud', 'a@x.test', 'A', 'x', 'active', 'br', 'br', 'br', 't', 't'),
               ('u-adm', 'd@x.test', 'D', 'x', 'active', 'br', 'br', 'br', 't', 't');
             INSERT INTO user_roles (user_id, role, granted_at, granted_by) VALUES
               ('u-cit', 'CITIZEN', 't1', NULL), ('u-aud', 'CITIZEN', 't2', NULL),
               ('u-aud', 'AUDITOR', 't3', 'admin-1'), ('u-adm', 'COUNTRY_ADMIN', 't4', 'operator:cli');",
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();

        assert_eq!(
            run(&db).await.unwrap(),
            vec!["0500_iam", "0520_performance"]
        );
        let conn = db.read().await.unwrap();
        let rows = super::super::db::query_all(
            &conn,
            "SELECT user_id, policy_id, attached_at, attached_by FROM iam_user_policies ORDER BY attached_at",
            (),
        )
        .await
        .unwrap();
        let got: Vec<[String; 4]> = rows
            .iter()
            .map(|r| std::array::from_fn(|i| r.get::<String>(i).unwrap()))
            .collect();
        let expect = [
            ["u-cit", "pol-cidadao", "t1", "migration:0500_iam"],
            ["u-aud", "pol-cidadao", "t2", "migration:0500_iam"],
            ["u-aud", "pol-auditor", "t3", "admin-1"],
            ["u-adm", "pol-administrator-access", "t4", "operator:cli"],
        ];
        assert_eq!(got, expect.map(|r| r.map(String::from)));
        assert!(
            super::super::db::query_opt(&conn, "SELECT 1 FROM user_roles", ())
                .await
                .is_err(),
            "roles table dropped"
        );
    }

    /// `0520_performance` backfills ballot counters from existing ballots.
    #[tokio::test]
    async fn performance_migration_backfills_ballot_counts() {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(&dir.path().join("counts.db")).await.unwrap();
        let tx = db.write().await.unwrap();
        tx.execute(
            "CREATE TABLE schema_migrations (version TEXT PRIMARY KEY, checksum TEXT NOT NULL, applied_at TEXT NOT NULL)",
            (),
        )
        .await
        .unwrap();
        for m in MIGRATIONS
            .iter()
            .take_while(|m| m.version != "0520_performance")
        {
            tx.execute_batch(m.sql).await.unwrap();
            tx.execute(
                "INSERT INTO schema_migrations VALUES (?1, ?2, 'before')",
                (m.version, sha256_hex(m.sql)),
            )
            .await
            .unwrap();
        }
        tx.commit().await.unwrap();
        // Minimal rows only: the snapshot's setup version is irrelevant here.
        let data = db.read().await.unwrap();
        data.execute("PRAGMA foreign_keys = OFF", ()).await.unwrap();
        data.execute_batch(
            "INSERT INTO jurisdictions (id, code, kind, name) VALUES ('br', 'BR', 'national', 'Brasil');
             INSERT INTO users (id, email, name, password_hash, status, municipality_id, state_id, country_id, created_at, updated_at) VALUES
               ('u1', '1@x.test', 'U', 'x', 'active', 'br', 'br', 'br', 't', 't'),
               ('u2', '2@x.test', 'U', 'x', 'active', 'br', 'br', 'br', 't', 't');
             INSERT INTO electorate_snapshots (id, proposal_id, phase, jurisdiction_id, setup_version_id, member_count, taken_at)
               VALUES ('s1', 'p1', 'voting', 'br', 'sv', 2, 't');
             INSERT INTO proposals (id, kind, jurisdiction_id, author_id, title, summary, body, normalized_title, normalized_body_hash, text_hash, status, created_at, updated_at, published_at) VALUES
               ('p1', 'urgent_law', 'br', 'u1', 'A', 'a', 'a', 'a', 'h', 'h', 'in_voting', 't', 't', 't'),
               ('p2', 'urgent_law', 'br', 'u1', 'B', 'b', 'b', 'b', 'h', 'h', 'in_voting', 't', 't', 't');
             INSERT INTO ballots (id, proposal_id, user_id, snapshot_id, choice, visibility, consented_at, cast_at) VALUES
               ('b1', 'p1', 'u1', 's1', 'yes', 'public', 't', 't'),
               ('b2', 'p1', 'u2', 's1', 'no', 'public', 't', 't');",
        )
        .await
        .unwrap();
        drop(data);
        assert_eq!(run(&db).await.unwrap(), vec!["0520_performance"]);
        let conn = db.read().await.unwrap();
        let rows = super::super::db::query_all(
            &conn,
            "SELECT proposal_id, ballots FROM ballot_counts ORDER BY proposal_id",
            (),
        )
        .await
        .unwrap();
        let got: Vec<(String, i64)> = rows
            .iter()
            .map(|r| (r.get(0).unwrap(), r.get(1).unwrap()))
            .collect();
        assert_eq!(got, vec![("p1".to_owned(), 2)]);
    }

    /// Expiry cleanups and territorial account lookups use an index.
    #[tokio::test]
    async fn cleanup_and_territory_queries_use_indexes() {
        let t = migrated().await;
        let conn = t.db.read().await.unwrap();
        for sql in [
            "DELETE FROM face_challenges WHERE expires_at < ?1",
            "DELETE FROM auth_challenges WHERE expires_at < ?1",
            "DELETE FROM auth_failures WHERE occurred_at <= ?1",
            "SELECT id FROM users WHERE country_id = ?1 AND status = 'active'",
            "SELECT id FROM users WHERE state_id = ?1 AND status = 'active'",
            "SELECT id FROM users WHERE municipality_id = ?1 AND status = 'active'",
        ] {
            let plan: Vec<String> =
                super::super::db::query_all(&conn, &format!("EXPLAIN QUERY PLAN {sql}"), ["x"])
                    .await
                    .unwrap()
                    .iter()
                    .map(|r| r.get::<String>(3).unwrap())
                    .collect();
            assert!(plan.iter().any(|p| p.contains("INDEX")), "{sql}: {plan:?}");
        }
    }

    #[tokio::test]
    async fn unknown_migration_is_rejected() {
        let t = migrated().await;
        let tx = t.db.write().await.unwrap();
        tx.execute(
            "INSERT INTO schema_migrations VALUES ('9999_future', 'x', 'now')",
            (),
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        assert!(matches!(run(&t.db).await, Err(MigrationError::Unknown(_))));
    }
}
