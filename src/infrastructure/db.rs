//! TursoDB access. One process owns the database; writes are serialized through a
//! single connection because concurrent `BEGIN IMMEDIATE` returns `Busy`.

use std::{path::Path, sync::Arc};

use tokio::sync::{Mutex, OwnedMutexGuard};
use turso::{Builder, Connection, Database, Row};

/// Transient lock contention (e.g. a checkpoint while a request reads) is
/// retried by the engine for up to this long before `Busy` reaches the caller.
const BUSY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

#[derive(Clone)]
pub struct Db {
    database: Database,
    writer: Arc<Mutex<Connection>>,
}

impl Db {
    pub async fn open(path: &Path) -> Result<Self, turso::Error> {
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent).map_err(|e| turso::Error::Misuse(e.to_string()))?;
        }
        let path = path
            .to_str()
            .ok_or_else(|| turso::Error::Misuse("non UTF-8 database path".into()))?;
        let database = Builder::new_local(path).build().await?;
        let writer = database.connect()?;
        writer.busy_timeout(BUSY_TIMEOUT)?;
        writer.execute("PRAGMA foreign_keys = ON", ()).await?;
        Ok(Self {
            database,
            writer: Arc::new(Mutex::new(writer)),
        })
    }

    /// New read connection with a consistent snapshot per statement/transaction.
    pub async fn read(&self) -> Result<Connection, turso::Error> {
        let conn = self.database.connect()?;
        conn.busy_timeout(BUSY_TIMEOUT)?;
        conn.execute("PRAGMA foreign_keys = ON", ()).await?;
        Ok(conn)
    }

    /// Starts an immediate write transaction on the single writer connection.
    /// A transaction dropped without commit is rolled back on the next acquisition.
    pub async fn write(&self) -> Result<WriteTx, turso::Error> {
        let conn = self.writer.clone().lock_owned().await;
        if !conn.is_autocommit()? {
            conn.execute("ROLLBACK", ()).await?;
        }
        conn.execute("BEGIN IMMEDIATE", ()).await?;
        Ok(WriteTx { conn })
    }
}

pub struct WriteTx {
    conn: OwnedMutexGuard<Connection>,
}

impl WriteTx {
    pub async fn commit(self) -> Result<(), turso::Error> {
        self.conn.execute("COMMIT", ()).await.map(|_| ())
    }

    pub async fn rollback(self) -> Result<(), turso::Error> {
        self.conn.execute("ROLLBACK", ()).await.map(|_| ())
    }
}

impl std::ops::Deref for WriteTx {
    type Target = Connection;

    fn deref(&self) -> &Connection {
        &self.conn
    }
}

/// Runs a query and returns the first row, if any.
pub async fn query_opt(
    conn: &Connection,
    sql: &str,
    params: impl turso::IntoParams,
) -> Result<Option<Row>, turso::Error> {
    let mut rows = conn.query(sql, params).await?;
    rows.next().await
}

/// Runs a query and collects all rows.
pub async fn query_all(
    conn: &Connection,
    sql: &str,
    params: impl turso::IntoParams,
) -> Result<Vec<Row>, turso::Error> {
    let mut rows = conn.query(sql, params).await?;
    let mut out = Vec::new();
    while let Some(row) = rows.next().await? {
        out.push(row);
    }
    Ok(out)
}

/// True when the error is a constraint violation (UNIQUE/CHECK/FK/trigger abort).
pub fn is_constraint(error: &turso::Error) -> bool {
    matches!(error, turso::Error::Constraint(_))
}

#[cfg(test)]
pub mod testing {
    use super::*;

    /// Disposable database in a temporary directory, migrated with the real engine.
    pub struct TestDb {
        pub db: Db,
        pub dir: tempfile::TempDir,
    }

    pub async fn migrated() -> TestDb {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = Db::open(&dir.path().join("test.db")).await.expect("open");
        crate::infrastructure::migrations::run(&db)
            .await
            .expect("migrate");
        TestDb { db, dir }
    }
}

#[cfg(test)]
mod concurrency_tests {
    use super::*;

    /// Regression: page loads issue several reads in parallel while background
    /// tasks (phase ticker, reminders, session touch) write. No read or write may
    /// fail with a transient lock error under that load.
    #[tokio::test(flavor = "multi_thread", worker_threads = 8)]
    async fn parallel_reads_and_writes_do_not_fail_with_busy() {
        let t = testing::migrated().await;
        let db = t.db.clone();
        let mut tasks = Vec::new();
        for w in 0..4 {
            let db = db.clone();
            tasks.push(tokio::spawn(async move {
                let mut errors = Vec::new();
                for i in 0..150 {
                    let r: Result<(), turso::Error> = async {
                        let tx = db.write().await?;
                        tx.execute(
                            "INSERT INTO synthetic_counter (id, value, updated_at) VALUES (1, 1, ?1)
                             ON CONFLICT(id) DO UPDATE SET value = value + 1, updated_at = excluded.updated_at",
                            [format!("w{w}-{i}")],
                        )
                        .await?;
                        tx.commit().await
                    }
                    .await;
                    if let Err(e) = r {
                        errors.push(format!("write: {e}"));
                    }
                }
                errors
            }));
        }
        for _ in 0..16 {
            let db = db.clone();
            tasks.push(tokio::spawn(async move {
                let mut errors = Vec::new();
                for _ in 0..150 {
                    let r: Result<(), turso::Error> = async {
                        let conn = db.read().await?;
                        // First row only: the statement stays open like `query_opt`.
                        let _ = query_opt(&conn, "SELECT value FROM synthetic_counter", ()).await?;
                        let _ =
                            query_all(&conn, "SELECT id FROM jurisdictions LIMIT 5", ()).await?;
                        Ok(())
                    }
                    .await;
                    if let Err(e) = r {
                        errors.push(format!("read: {e}"));
                    }
                }
                errors
            }));
        }
        let mut errors = Vec::new();
        for task in tasks {
            errors.extend(task.await.unwrap());
        }
        assert!(
            errors.is_empty(),
            "{} errors, first: {:?}",
            errors.len(),
            errors.first()
        );
    }
}
