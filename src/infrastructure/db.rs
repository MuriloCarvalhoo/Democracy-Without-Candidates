//! TursoDB access. One process owns the database; writes are serialized through a
//! single connection because concurrent `BEGIN IMMEDIATE` returns `Busy`.
//! Reads use pooled connections: in WAL mode a read sees the last committed
//! snapshot and never waits for the writer (no dirty reads).

use std::{
    path::Path,
    sync::{Arc, Mutex as StdMutex},
};

use tokio::sync::{Mutex, OwnedMutexGuard};
use turso::{Builder, Connection, Database, Row};

/// Transient lock contention (e.g. a checkpoint while a request reads) is
/// retried by the engine for up to this long before `Busy` reaches the caller.
const BUSY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// Idle read connections kept for reuse. Not a concurrency limit: when the pool
/// is empty a new connection is opened, and extras are closed on return.
const MAX_IDLE_READERS: usize = 32;

#[derive(Clone)]
pub struct Db {
    database: Database,
    writer: Arc<Mutex<Connection>>,
    readers: Arc<StdMutex<Vec<Connection>>>,
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
            readers: Arc::new(StdMutex::new(Vec::with_capacity(MAX_IDLE_READERS))),
        })
    }

    /// Read connection with a consistent snapshot per statement/transaction,
    /// reused from the pool when one is idle. Rows must not outlive the guard.
    pub async fn read(&self) -> Result<ReadConn, turso::Error> {
        let pooled = self.readers.lock().ok().and_then(|mut idle| idle.pop());
        let conn = match pooled {
            Some(conn) => conn,
            None => {
                let conn = self.database.connect()?;
                conn.busy_timeout(BUSY_TIMEOUT)?;
                conn.execute("PRAGMA foreign_keys = ON", ()).await?;
                conn
            }
        };
        Ok(ReadConn {
            conn: Some(conn),
            pool: self.readers.clone(),
        })
    }

    /// New connection outside the pool, for statements that must not be
    /// re-executed after a schema re-preparation on a long-lived connection
    /// (`VACUUM INTO` fails when its output file already exists).
    pub async fn read_unpooled(&self) -> Result<Connection, turso::Error> {
        let conn = self.database.connect()?;
        conn.busy_timeout(BUSY_TIMEOUT)?;
        Ok(conn)
    }

    #[cfg(test)]
    fn idle_readers(&self) -> usize {
        self.readers.lock().map(|idle| idle.len()).unwrap_or(0)
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

/// Pooled read connection; returned to the pool on drop only when it is back in
/// autocommit mode (no open transaction), otherwise closed.
pub struct ReadConn {
    conn: Option<Connection>,
    pool: Arc<StdMutex<Vec<Connection>>>,
}

impl std::ops::Deref for ReadConn {
    type Target = Connection;

    fn deref(&self) -> &Connection {
        self.conn.as_ref().expect("connection present until drop")
    }
}

impl Drop for ReadConn {
    fn drop(&mut self) {
        let Some(conn) = self.conn.take() else { return };
        if !matches!(conn.is_autocommit(), Ok(true)) {
            return;
        }
        if let Ok(mut idle) = self.pool.lock()
            && idle.len() < MAX_IDLE_READERS
        {
            idle.push(conn);
        }
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
        assert!(
            db.idle_readers() <= MAX_IDLE_READERS,
            "idle pool stays bounded"
        );
        assert!(db.idle_readers() > 0, "read connections are reused");
    }

    async fn counter(conn: &Connection) -> Option<i64> {
        query_opt(conn, "SELECT value FROM synthetic_counter WHERE id = 1", ())
            .await
            .unwrap()
            .map(|r| r.get(0).unwrap())
    }

    async fn bump(db: &Db) {
        let tx = db.write().await.unwrap();
        tx.execute(
            "INSERT INTO synthetic_counter (id, value, updated_at) VALUES (1, 1, 't')
             ON CONFLICT(id) DO UPDATE SET value = value + 1",
            (),
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
    }

    /// A reused connection must not keep an old snapshot: after a partially read
    /// statement (`query_opt`) it sees writes committed in the meantime.
    #[tokio::test]
    async fn reused_connection_sees_later_commits() {
        let t = testing::migrated().await;
        bump(&t.db).await;
        {
            let conn = t.db.read().await.unwrap();
            let _ = query_opt(&conn, "SELECT id FROM jurisdictions UNION ALL SELECT 1", ())
                .await
                .unwrap();
            assert_eq!(counter(&conn).await, Some(1));
        }
        assert_eq!(t.db.idle_readers(), 1);
        bump(&t.db).await;
        let conn = t.db.read().await.unwrap();
        assert_eq!(t.db.idle_readers(), 0, "the pooled connection was reused");
        assert_eq!(counter(&conn).await, Some(2));
    }

    /// A connection dropped inside a transaction is closed, never returned dirty.
    #[tokio::test]
    async fn connection_mid_transaction_is_not_returned() {
        let t = testing::migrated().await;
        bump(&t.db).await;
        {
            let conn = t.db.read().await.unwrap();
            conn.execute("BEGIN", ()).await.unwrap();
            assert_eq!(counter(&conn).await, Some(1));
        }
        assert_eq!(t.db.idle_readers(), 0, "dirty connection discarded");
        bump(&t.db).await;
        let conn = t.db.read().await.unwrap();
        assert!(conn.is_autocommit().unwrap());
        assert_eq!(counter(&conn).await, Some(2));
    }
}
