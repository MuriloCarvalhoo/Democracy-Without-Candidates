//! Client action keys stored with the effect in the same transaction. Same key
//! and payload return the stored result; same key with another payload conflicts.

use chrono::{DateTime, Utc};
use serde::{Serialize, de::DeserializeOwned};
use turso::Connection;

use crate::{
    error::AppError,
    infrastructure::{clock::ts, crypto::sha256_hex, db::query_opt},
};

pub struct Key<'a> {
    pub user_id: &'a str,
    pub key: &'a str,
    pub action: &'a str,
    pub request_hash: String,
}

/// Validates an optional client key: 8–128 visible ASCII characters.
pub fn parse(raw: Option<&str>) -> Result<Option<&str>, AppError> {
    match raw.map(str::trim).filter(|k| !k.is_empty()) {
        None => Ok(None),
        Some(k) if (8..=128).contains(&k.len()) && k.bytes().all(|b| b.is_ascii_graphic()) => {
            Ok(Some(k))
        }
        Some(_) => Err(AppError::validation("idempotency_key", "chave inválida")),
    }
}

pub fn request_hash(parts: &[&str]) -> String {
    sha256_hex(serde_json::to_string(parts).unwrap_or_default())
}

/// Returns the stored response for a replay, or `None` for a new key.
#[tracing::instrument(level = "debug", skip_all, err(level = "debug"))]
pub async fn lookup<T: DeserializeOwned>(
    conn: &Connection,
    key: &Key<'_>,
) -> Result<Option<T>, AppError> {
    let Some(row) = query_opt(
        conn,
        "SELECT action, request_hash, response FROM idempotency_keys WHERE user_id = ?1 AND key = ?2",
        (key.user_id, key.key),
    )
    .await?
    else {
        return Ok(None);
    };
    let (action, hash, response): (String, String, String) =
        (row.get(0)?, row.get(1)?, row.get(2)?);
    if action != key.action || hash != key.request_hash {
        return Err(AppError::Conflict(
            crate::i18n::t("proposals.error.idempotency_conflict").into(),
        ));
    }
    serde_json::from_str(&response)
        .map(Some)
        .map_err(AppError::internal)
}

#[tracing::instrument(level = "debug", skip_all, err(level = "debug"))]
pub async fn store<T: Serialize>(
    conn: &Connection,
    now: DateTime<Utc>,
    key: &Key<'_>,
    response: &T,
) -> Result<(), AppError> {
    let body = serde_json::to_string(response).map_err(AppError::internal)?;
    conn.execute(
        "INSERT INTO idempotency_keys (user_id, key, action, request_hash, response, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        (key.user_id, key.key, key.action, key.request_hash.as_str(), body, ts(now)),
    )
    .await?;
    Ok(())
}
