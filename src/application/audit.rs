//! Append-only audit events with a local SHA-256 chain. Payloads must be minimal:
//! never passwords, secrets, biometrics, IPs or anything linking a private ballot.

use serde_json::Value;
use turso::Connection;

use crate::{
    error::AppError,
    infrastructure::{
        clock::ts,
        crypto::sha256_hex,
        db::{query_all, query_opt},
    },
};

pub const GENESIS: &str = "0000000000000000000000000000000000000000000000000000000000000000";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visibility {
    Public,
    Restricted,
}

impl Visibility {
    fn as_db(self) -> &'static str {
        match self {
            Self::Public => "public",
            Self::Restricted => "restricted",
        }
    }
}

pub struct Event<'a> {
    pub kind: &'a str,
    pub visibility: Visibility,
    pub actor_ref: Option<&'a str>,
    pub subject: Option<&'a str>,
    pub payload: Value,
}

/// Hash of the previous hash followed by the stored fields
/// `[seq, occurred_at, kind, visibility, actor_ref, subject, payload]`.
fn chain_hash(prev: &str, fields: Value) -> String {
    sha256_hex(serde_json::json!([prev, fields]).to_string())
}

/// Appends inside the caller's write transaction so the event commits with the effect.
pub async fn append(
    conn: &Connection,
    now: chrono::DateTime<chrono::Utc>,
    event: Event<'_>,
) -> Result<i64, AppError> {
    let (prev_seq, prev_hash) = match query_opt(
        conn,
        "SELECT seq, hash FROM audit_events ORDER BY seq DESC LIMIT 1",
        (),
    )
    .await?
    {
        Some(row) => (row.get::<i64>(0)?, row.get::<String>(1)?),
        None => (0, GENESIS.to_owned()),
    };
    let seq = prev_seq + 1;
    let at = ts(now);
    let payload = event.payload.to_string();
    let vis = event.visibility.as_db();
    let hash = chain_hash(
        &prev_hash,
        serde_json::json!([
            seq,
            at,
            event.kind,
            vis,
            event.actor_ref,
            event.subject,
            payload
        ]),
    );
    conn.execute(
        "INSERT INTO audit_events (seq, occurred_at, kind, visibility, actor_ref, subject, payload, prev_hash, hash)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        (seq, at, event.kind, vis, event.actor_ref, event.subject, payload, prev_hash, hash),
    )
    .await?;
    Ok(seq)
}

#[derive(Debug, PartialEq, Eq)]
pub enum ChainStatus {
    Valid { events: i64, head: String },
    Broken { at_seq: i64 },
}

/// Recomputes the whole chain. Detects edits/gaps in the local copy only.
pub async fn verify(conn: &Connection) -> Result<ChainStatus, AppError> {
    let rows = query_all(
        conn,
        "SELECT seq, occurred_at, kind, visibility, actor_ref, subject, payload, prev_hash, hash FROM audit_events ORDER BY seq",
        (),
    )
    .await?;
    let mut prev = GENESIS.to_owned();
    for (expected_seq, row) in (1..).zip(rows.iter()) {
        let seq: i64 = row.get(0)?;
        let prev_hash: String = row.get(7)?;
        let hash: String = row.get(8)?;
        let recomputed = chain_hash(
            &prev,
            serde_json::json!([
                seq,
                row.get::<String>(1)?,
                row.get::<String>(2)?,
                row.get::<String>(3)?,
                row.get::<Option<String>>(4)?,
                row.get::<Option<String>>(5)?,
                row.get::<String>(6)?,
            ]),
        );
        if seq != expected_seq || prev_hash != prev || recomputed != hash {
            return Ok(ChainStatus::Broken { at_seq: seq });
        }
        prev = hash;
    }
    Ok(ChainStatus::Valid {
        events: rows.len() as i64,
        head: prev,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::db::testing::migrated;
    use serde_json::json;

    #[tokio::test]
    async fn chain_appends_verifies_and_is_append_only() {
        let t = migrated().await;
        let now = chrono::Utc::now();
        let tx = t.db.write().await.unwrap();
        for i in 0..3 {
            append(
                &tx,
                now,
                Event {
                    kind: "test.event",
                    visibility: Visibility::Public,
                    actor_ref: None,
                    subject: Some("s"),
                    payload: json!({ "i": i }),
                },
            )
            .await
            .unwrap();
        }
        tx.commit().await.unwrap();
        let conn = t.db.read().await.unwrap();
        assert!(matches!(
            verify(&conn).await.unwrap(),
            ChainStatus::Valid { events: 3, .. }
        ));
        assert!(
            conn.execute("UPDATE audit_events SET payload = '{}' WHERE seq = 2", ())
                .await
                .is_err()
        );
        assert!(
            conn.execute("DELETE FROM audit_events WHERE seq = 3", ())
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn rolled_back_effect_leaves_no_event() {
        let t = migrated().await;
        let tx = t.db.write().await.unwrap();
        append(
            &tx,
            chrono::Utc::now(),
            Event {
                kind: "x",
                visibility: Visibility::Restricted,
                actor_ref: None,
                subject: None,
                payload: json!({}),
            },
        )
        .await
        .unwrap();
        tx.rollback().await.unwrap();
        let conn = t.db.read().await.unwrap();
        assert_eq!(
            verify(&conn).await.unwrap(),
            ChainStatus::Valid {
                events: 0,
                head: GENESIS.into()
            }
        );
    }
}
