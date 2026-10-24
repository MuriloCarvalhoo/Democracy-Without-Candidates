//! Instance feature flags: read once at startup; database changes apply after restart.
//! Unknown keys in the database are ignored; missing keys disable optional features.

use std::collections::BTreeMap;

use turso::Connection;

use super::{
    clock::ts,
    db::{Db, query_all},
};
use crate::error::AppError;

pub struct FlagDef {
    pub key: &'static str,
    pub description: &'static str,
}

pub const ADDRESS_BRASILAPI: &str = "address_provider.brasilapi";
pub const PROPOSALS: &str = "proposals";
pub const SIGNATURES: &str = "signatures";
pub const DEBATE: &str = "debate";
pub const VOTING: &str = "voting";
pub const VOTING_PRIVATE: &str = "voting.private";

pub const CATALOG: &[FlagDef] = &[
    FlagDef {
        key: ADDRESS_BRASILAPI,
        description: "Busca de endereço por CEP via BrasilAPI (somente Brasil)",
    },
    FlagDef {
        key: PROPOSALS,
        description: "Criação e publicação de propostas (exige setup adotado)",
    },
    FlagDef {
        key: SIGNATURES,
        description: "Coleta de assinaturas",
    },
    FlagDef {
        key: DEBATE,
        description: "Comentários, emendas e moderação",
    },
    FlagDef {
        key: VOTING,
        description: "Votação demonstrativa pública com dados sintéticos",
    },
    FlagDef {
        key: VOTING_PRIVATE,
        description: "Voto privado: bloqueado até protocolo validado",
    },
];

/// Flags that cannot be enabled by this build regardless of the database value.
const NOT_IMPLEMENTED: &[&str] = &[VOTING_PRIVATE];

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct Flags {
    enabled: BTreeMap<&'static str, bool>,
}

impl Flags {
    pub async fn load(db: &Db) -> Result<Self, turso::Error> {
        let conn = db.read().await?;
        let rows = query_all(&conn, "SELECT key, enabled FROM feature_flags", ()).await?;
        let mut enabled: BTreeMap<&'static str, bool> =
            CATALOG.iter().map(|d| (d.key, false)).collect();
        for row in rows {
            let key: String = row.get(0)?;
            let on: i64 = row.get(1)?;
            match CATALOG.iter().find(|d| d.key == key) {
                Some(def) if NOT_IMPLEMENTED.contains(&def.key) => {
                    if on == 1 {
                        tracing::warn!(
                            flag = def.key,
                            "flag enabled in database but not implemented; kept disabled"
                        );
                    }
                }
                Some(def) => {
                    enabled.insert(def.key, on == 1);
                }
                None => tracing::warn!(%key, "unknown feature flag ignored"),
            }
        }
        Ok(Self { enabled })
    }

    pub fn is_enabled(&self, key: &str) -> bool {
        self.enabled.get(key).copied().unwrap_or(false)
    }

    pub fn require(&self, key: &'static str) -> Result<(), AppError> {
        if self.is_enabled(key) {
            Ok(())
        } else {
            Err(AppError::FeatureDisabled(key.into()))
        }
    }

    pub fn enabled_keys(&self) -> Vec<&'static str> {
        self.enabled
            .iter()
            .filter(|(_, on)| **on)
            .map(|(k, _)| *k)
            .collect()
    }

    #[cfg(test)]
    pub fn with(keys: &[&'static str]) -> Self {
        let mut flags = Self::default();
        for d in CATALOG {
            flags.enabled.insert(d.key, keys.contains(&d.key));
        }
        flags
    }
}

/// True when this build can honour the flag (false for blocked features).
pub fn is_implemented(key: &str) -> bool {
    !NOT_IMPLEMENTED.contains(&key)
}

/// Values currently stored in the database (may differ from the effective state).
pub async fn stored(conn: &Connection) -> Result<BTreeMap<String, bool>, turso::Error> {
    let rows = query_all(conn, "SELECT key, enabled FROM feature_flags", ()).await?;
    let mut out = BTreeMap::new();
    for row in rows {
        out.insert(row.get::<String>(0)?, row.get::<i64>(1)? == 1);
    }
    Ok(out)
}

/// Persists a flag value inside the caller's write transaction. Takes effect only
/// after the application restarts. Unknown keys and enabling a blocked feature fail.
pub async fn store_in(
    conn: &Connection,
    now: chrono::DateTime<chrono::Utc>,
    key: &str,
    enabled: bool,
    by: Option<&str>,
) -> Result<(), AppError> {
    if !CATALOG.iter().any(|d| d.key == key) {
        return Err(AppError::validation("key", "flag desconhecida"));
    }
    if enabled && !is_implemented(key) {
        return Err(AppError::validation(
            "key",
            "funcionalidade não implementada",
        ));
    }
    conn.execute(
        "INSERT INTO feature_flags (key, enabled, updated_at, updated_by) VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(key) DO UPDATE SET enabled = excluded.enabled, updated_at = excluded.updated_at, updated_by = excluded.updated_by",
        (key, enabled as i64, ts(now), by),
    )
    .await?;
    Ok(())
}

/// Persists a flag value in its own transaction (fixtures/CLI).
pub async fn store(db: &Db, key: &str, enabled: bool, by: Option<&str>) -> Result<(), AppError> {
    let tx = db.write().await?;
    store_in(&tx, chrono::Utc::now(), key, enabled, by).await?;
    tx.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::db::testing::migrated;

    #[tokio::test]
    async fn missing_flags_are_disabled_and_changes_need_reload() {
        let t = migrated().await;
        let flags = Flags::load(&t.db).await.unwrap();
        assert!(!flags.is_enabled(PROPOSALS));
        store(&t.db, PROPOSALS, true, None).await.unwrap();
        assert!(
            !flags.is_enabled(PROPOSALS),
            "running state does not change"
        );
        assert!(Flags::load(&t.db).await.unwrap().is_enabled(PROPOSALS));
    }

    #[tokio::test]
    async fn unknown_and_unimplemented_flags_stay_off() {
        let t = migrated().await;
        let tx = t.db.write().await.unwrap();
        tx.execute(
            "INSERT INTO feature_flags VALUES ('admin.bypass', 1, 'now', NULL)",
            (),
        )
        .await
        .unwrap();
        tx.execute(
            "INSERT INTO feature_flags VALUES ('voting.private', 1, 'now', NULL)",
            (),
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        let flags = Flags::load(&t.db).await.unwrap();
        assert!(!flags.is_enabled("admin.bypass"));
        assert!(!flags.is_enabled(VOTING_PRIVATE));
        assert!(store(&t.db, "admin.bypass", true, None).await.is_err());
        assert!(store(&t.db, VOTING_PRIVATE, true, None).await.is_err());
        assert!(store(&t.db, VOTING_PRIVATE, false, None).await.is_ok());
    }
}
