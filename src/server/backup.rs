//! Consistent database snapshots and read-only verification.
//!
//! TursoDB locks the database file for the owning process, so another process cannot
//! open a running instance's database, and copying the files while it runs is unsafe
//! (recent commits live in the WAL). Snapshots therefore use the engine's
//! `VACUUM INTO` from a connection of the process that owns the database: the
//! running server (SIGUSR1) or `dwc backup` on a stopped instance.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use serde::Serialize;

use crate::{
    application::{
        audit::{self, ChainStatus},
        voting,
    },
    infrastructure::{
        db::{Db, query_all, query_opt},
        migrations,
    },
};

#[derive(Debug, thiserror::Error)]
pub enum BackupError {
    #[error("database: {0}")]
    Db(#[from] turso::Error),
    #[error("{0}")]
    Invalid(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

/// Writes a consistent copy to `dest` (must not exist). The copy is built in a
/// temporary sibling file and renamed only after the engine reports success.
pub async fn snapshot(db: &Db, dest: &Path) -> Result<(), BackupError> {
    if dest.exists() {
        return Err(BackupError::Invalid(format!(
            "{} already exists",
            dest.display()
        )));
    }
    let tmp = dest.with_extension("partial");
    let tmp_str = tmp
        .to_str()
        .filter(|s| !s.contains('\''))
        .ok_or_else(|| BackupError::Invalid("unsupported destination path".into()))?;
    if let Some(parent) = dest.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }
    // The engine may leave an (empty, already checkpointed) WAL next to the copy.
    let wal = PathBuf::from(format!("{tmp_str}-wal"));
    let _ = std::fs::remove_file(&tmp);
    let _ = std::fs::remove_file(&wal);
    let conn = db.read().await?;
    // VACUUM INTO takes a literal path only (no bound parameters).
    let result = conn.execute(&format!("VACUUM INTO '{tmp_str}'"), ()).await;
    drop(conn);
    let wal_bytes = std::fs::metadata(&wal).map(|m| m.len()).unwrap_or(0);
    let _ = std::fs::remove_file(&wal);
    if let Err(e) = result {
        let _ = std::fs::remove_file(&tmp);
        return Err(e.into());
    }
    if wal_bytes > 0 {
        // Never ship a main file whose last pages may live only in a WAL.
        let _ = std::fs::remove_file(&tmp);
        return Err(BackupError::Invalid(
            "engine left a non-empty WAL for the copy; snapshot discarded".into(),
        ));
    }
    std::fs::rename(&tmp, dest)?;
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DbReport {
    pub integrity: Vec<String>,
    pub migrations_applied: Vec<String>,
    pub migrations_pending: Vec<String>,
    pub audit_chain: String,
    pub synthetic_counter: i64,
    pub voting: voting::VotingCheck,
    pub tables: BTreeMap<String, i64>,
}

impl DbReport {
    /// Integrity ok, no pending/unknown migrations and an unbroken audit chain.
    pub fn healthy(&self) -> bool {
        self.integrity == ["ok"]
            && self.migrations_pending.is_empty()
            && self.audit_chain.starts_with("valid")
            && self.voting.healthy()
    }
}

/// Read-only verification: engine integrity check, migration state for this
/// binary, audit chain, voting invariants and row counts per table. Fails on unknown/edited migrations.
pub async fn check(db: &Db) -> Result<DbReport, BackupError> {
    let conn = db.read().await?;
    let mut integrity = Vec::new();
    for row in query_all(&conn, "PRAGMA integrity_check", ()).await? {
        integrity.push(row.get::<String>(0)?);
    }
    let (applied, pending) = migrations::status(&conn)
        .await
        .map_err(|e| BackupError::Invalid(format!("migrations: {e}")))?;
    let audit_chain = match audit::verify(&conn)
        .await
        .map_err(|e| BackupError::Invalid(format!("audit chain: {e}")))?
    {
        ChainStatus::Valid { events, head } => format!("valid:{events}:{head}"),
        ChainStatus::Broken { at_seq } => format!("broken:{at_seq}"),
    };
    let synthetic_counter = match query_opt(
        &conn,
        "SELECT value FROM synthetic_counter WHERE id = 1",
        (),
    )
    .await?
    {
        Some(row) => row.get(0)?,
        None => 0,
    };
    let mut tables = BTreeMap::new();
    for row in query_all(
        &conn,
        "SELECT name FROM sqlite_schema WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
        (),
    )
    .await?
    {
        let name: String = row.get(0)?;
        if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return Err(BackupError::Invalid(format!("unexpected table name {name}")));
        }
        let count: i64 = query_opt(&conn, &format!("SELECT COUNT(*) FROM \"{name}\""), ())
            .await?
            .ok_or_else(|| BackupError::Invalid("count".into()))?
            .get(0)?;
        tables.insert(name, count);
    }
    Ok(DbReport {
        integrity,
        migrations_applied: applied,
        migrations_pending: pending.into_iter().map(String::from).collect(),
        audit_chain,
        synthetic_counter,
        voting: voting::integrity(&conn).await?,
        tables,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        application::audit::{Event, Visibility, append},
        infrastructure::{clock::ts, db::testing::migrated},
    };

    async fn increment(db: &Db) {
        let tx = db.write().await.unwrap();
        tx.execute(
            "INSERT INTO synthetic_counter (id, value, updated_at) VALUES (1, 1, ?1)
             ON CONFLICT(id) DO UPDATE SET value = value + 1",
            [ts(chrono::Utc::now())],
        )
        .await
        .unwrap();
        append(
            &tx,
            chrono::Utc::now(),
            Event {
                kind: "test.increment",
                visibility: Visibility::Restricted,
                actor_ref: None,
                subject: None,
                payload: serde_json::json!({}),
            },
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
    }

    #[tokio::test]
    async fn snapshot_restores_identical_verified_copy() {
        let t = migrated().await;
        for _ in 0..25 {
            increment(&t.db).await;
        }
        let original = check(&t.db).await.unwrap();
        assert!(original.healthy(), "{original:?}");
        assert_eq!(original.synthetic_counter, 25);
        assert_eq!(original.tables["audit_events"], 25);

        let dest = t.dir.path().join("snap/copy.db");
        snapshot(&t.db, &dest).await.unwrap();
        assert!(!dest.with_extension("partial").exists());
        assert!(!t.dir.path().join("snap/copy.partial-wal").exists());
        assert!(
            snapshot(&t.db, &dest).await.is_err(),
            "never overwrites an existing file"
        );
        let restored = Db::open(&dest).await.unwrap();
        assert_eq!(check(&restored).await.unwrap(), original);
    }

    #[tokio::test]
    async fn snapshot_during_writes_is_consistent() {
        let t = migrated().await;
        increment(&t.db).await;
        let writer = {
            let db = t.db.clone();
            tokio::spawn(async move {
                for _ in 0..200 {
                    increment(&db).await;
                }
            })
        };
        let dest = t.dir.path().join("live.db");
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        snapshot(&t.db, &dest).await.unwrap();
        writer.await.unwrap();
        let copy = check(&Db::open(&dest).await.unwrap()).await.unwrap();
        assert!(copy.healthy(), "{copy:?}");
        // Counter and audit events are written in the same transaction: a consistent
        // snapshot has exactly as many events as increments.
        assert_eq!(copy.synthetic_counter, copy.tables["audit_events"]);
        assert!((1..=201).contains(&copy.synthetic_counter));
    }

    #[tokio::test]
    async fn rejects_unsafe_destination() {
        let t = migrated().await;
        assert!(
            snapshot(&t.db, &t.dir.path().join("it's.db"))
                .await
                .is_err()
        );
    }
}
