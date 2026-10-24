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
    migration!("0450_audit_notifications"),
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

    let applied_versions = verified_applied(&db.read().await?).await?;
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
