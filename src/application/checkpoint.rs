//! Signed audit checkpoints: "the chain of this instance had head H at seq N".
//! They detect a rewrite of the local chain only when the exported JSON is kept
//! outside the VPS (an operator with the database and key can sign a new history).

use chrono::{DateTime, Utc};
use ed25519_dalek::SigningKey;
use turso::Connection;

use super::audit::{ChainStatus, GENESIS, verify as verify_chain, walk};
use crate::{
    domain::audit::Checkpoint,
    error::AppError,
    infrastructure::{
        clock::ts,
        crypto::new_id,
        db::{Db, query_all},
        signing,
    },
};

/// Verifies the full chain, signs its head and records the checkpoint row.
#[tracing::instrument(level = "debug", skip_all, fields(country = %country), err(level = "debug"))]
pub async fn create(
    db: &Db,
    key: &SigningKey,
    country: &str,
    now: DateTime<Utc>,
) -> Result<Checkpoint, AppError> {
    let tx = db.write().await?;
    let (seq, head_hash) = match verify_chain(&tx).await? {
        ChainStatus::Valid { events: 0, .. } => {
            return Err(AppError::Conflict("audit chain is empty".into()));
        }
        ChainStatus::Valid { events, head } => (events, head),
        ChainStatus::Broken { at_seq } => {
            return Err(AppError::Conflict(format!(
                "audit chain is broken at seq {at_seq}; refusing to sign"
            )));
        }
    };
    let created_at = ts(now);
    let message = Checkpoint::message(country, seq, &head_hash, &created_at);
    let checkpoint = Checkpoint {
        country: country.to_owned(),
        seq,
        head_hash,
        created_at,
        public_key: signing::public_key_hex(key),
        signature: signing::sign_hex(key, &message),
    };
    tx.execute(
        "INSERT INTO audit_checkpoints (id, seq, head_hash, created_at, public_key, signature) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        (
            new_id(),
            checkpoint.seq,
            checkpoint.head_hash.as_str(),
            checkpoint.created_at.as_str(),
            checkpoint.public_key.as_str(),
            checkpoint.signature.as_str(),
        ),
    )
    .await?;
    tx.commit().await?;
    Ok(checkpoint)
}

#[derive(Debug, PartialEq, Eq)]
pub enum CheckpointCheck {
    /// Signature valid and the local chain still has `head_hash` at `seq`.
    Valid {
        key_is_local: bool,
    },
    BadSignature,
    WrongCountry,
    /// Local chain is shorter than the checkpoint (events removed/reset).
    MissingSeq {
        local_events: i64,
    },
    /// Local chain has a different history up to `seq` (rewritten).
    HeadMismatch {
        local_head: String,
    },
    /// Local chain does not even verify up to `seq`.
    ChainBroken {
        at_seq: i64,
    },
}

#[tracing::instrument(level = "debug", skip_all, fields(country = %country), err(level = "debug"))]
pub async fn verify(
    conn: &Connection,
    checkpoint: &Checkpoint,
    country: &str,
    local_public_key: Option<&str>,
) -> Result<CheckpointCheck, AppError> {
    if !signing::verify_hex(
        &checkpoint.public_key,
        &checkpoint.signed_message(),
        &checkpoint.signature,
    ) {
        return Ok(CheckpointCheck::BadSignature);
    }
    if checkpoint.country != country {
        return Ok(CheckpointCheck::WrongCountry);
    }
    Ok(
        match walk(conn, 0, GENESIS, checkpoint.seq, &mut Default::default()).await? {
            ChainStatus::Broken { at_seq } => CheckpointCheck::ChainBroken { at_seq },
            ChainStatus::Valid { events, .. } if events < checkpoint.seq => {
                CheckpointCheck::MissingSeq {
                    local_events: events,
                }
            }
            ChainStatus::Valid { head, .. } if head != checkpoint.head_hash => {
                CheckpointCheck::HeadMismatch { local_head: head }
            }
            ChainStatus::Valid { .. } => CheckpointCheck::Valid {
                key_is_local: local_public_key == Some(checkpoint.public_key.as_str()),
            },
        },
    )
}

/// Most recent recorded checkpoints, newest first.
#[tracing::instrument(level = "debug", skip_all, fields(country = %country, limit = %limit), err(level = "debug"))]
pub async fn latest(
    conn: &Connection,
    country: &str,
    limit: u32,
) -> Result<Vec<Checkpoint>, AppError> {
    query_all(
        conn,
        &format!(
            "SELECT seq, head_hash, created_at, public_key, signature FROM audit_checkpoints ORDER BY created_at DESC, seq DESC LIMIT {limit}"
        ),
        (),
    )
    .await?
    .into_iter()
    .map(|r| {
        Ok(Checkpoint {
            country: country.to_owned(),
            seq: r.get(0)?,
            head_hash: r.get(1)?,
            created_at: r.get(2)?,
            public_key: r.get(3)?,
            signature: r.get(4)?,
        })
    })
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        application::audit::{Event, Visibility, append},
        infrastructure::db::testing::{TestDb, migrated},
    };

    async fn chain(payloads: &[&str]) -> TestDb {
        let t = migrated().await;
        let now: DateTime<Utc> = "2026-10-24T12:00:00Z".parse().unwrap();
        let tx = t.db.write().await.unwrap();
        for p in payloads {
            append(
                &tx,
                now,
                Event {
                    kind: "test.event",
                    visibility: Visibility::Public,
                    actor_ref: None,
                    subject: None,
                    payload: serde_json::json!({ "v": p }),
                },
            )
            .await
            .unwrap();
        }
        tx.commit().await.unwrap();
        t
    }

    fn key() -> SigningKey {
        SigningKey::from_bytes(&[7u8; 32])
    }

    #[tokio::test]
    async fn checkpoint_signs_head_and_detects_rewrite_after_export() {
        let now = Utc::now();
        let original = chain(&["a", "b", "c"]).await;
        assert!(matches!(
            create(&chain(&[]).await.db, &key(), "BR", now).await,
            Err(AppError::Conflict(_))
        ));
        let cp = create(&original.db, &key(), "BR", now).await.unwrap();
        assert_eq!(cp.seq, 3);
        let pk = signing::public_key_hex(&key());
        let conn = original.db.read().await.unwrap();
        assert_eq!(
            latest(&conn, "BR", 5).await.unwrap(),
            vec![cp.clone()],
            "row recorded"
        );
        assert_eq!(
            verify(&conn, &cp, "BR", Some(&pk)).await.unwrap(),
            CheckpointCheck::Valid { key_is_local: true }
        );

        // The chain keeps growing: an older checkpoint still verifies.
        let tx = original.db.write().await.unwrap();
        append(
            &tx,
            now,
            Event {
                kind: "test.event",
                visibility: Visibility::Restricted,
                actor_ref: None,
                subject: None,
                payload: serde_json::json!({}),
            },
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        assert_eq!(
            verify(&conn, &cp, "BR", None).await.unwrap(),
            CheckpointCheck::Valid {
                key_is_local: false
            }
        );

        // Same length, different history: a rewritten chain is internally valid
        // but no longer contains the exported head.
        let rewritten = chain(&["a", "B", "c"]).await;
        let rconn = rewritten.db.read().await.unwrap();
        assert!(matches!(
            verify(&rconn, &cp, "BR", Some(&pk)).await.unwrap(),
            CheckpointCheck::HeadMismatch { .. }
        ));
        let truncated = chain(&["a"]).await;
        assert_eq!(
            verify(&truncated.db.read().await.unwrap(), &cp, "BR", Some(&pk))
                .await
                .unwrap(),
            CheckpointCheck::MissingSeq { local_events: 1 }
        );

        // Tampered checkpoint fields invalidate the signature.
        let mut forged = cp.clone();
        forged.seq = 1;
        assert_eq!(
            verify(&conn, &forged, "BR", Some(&pk)).await.unwrap(),
            CheckpointCheck::BadSignature
        );
        let mut forged = cp.clone();
        forged.public_key = signing::public_key_hex(&SigningKey::from_bytes(&[8u8; 32]));
        assert_eq!(
            verify(&conn, &forged, "BR", Some(&pk)).await.unwrap(),
            CheckpointCheck::BadSignature
        );
        assert_eq!(
            verify(&conn, &cp, "PT", Some(&pk)).await.unwrap(),
            CheckpointCheck::WrongCountry
        );
        assert!(
            conn.execute("DELETE FROM audit_checkpoints", ())
                .await
                .is_err(),
            "checkpoint rows are append-only"
        );
    }
}
