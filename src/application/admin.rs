//! Instance administration use cases: feature flags and instance health. Every
//! operation checks the IAM permission here; the panel only reflects the result.
//! Accreditation (auditors, moderators, admins) is IAM policy attachment
//! (`application::iam`).
//! Flag changes are stored for the next restart; nothing here restarts the process.

use std::path::Path;

use chrono::{DateTime, Utc};
use serde_json::json;
use turso::Connection;

use super::{
    actor::Actor,
    audit::{self, Event, Visibility},
};
use crate::{
    domain::{
        admin::{AdminOverview, FlagView, InstanceStats, pending_restart},
        identity::Permission,
    },
    error::AppError,
    infrastructure::{
        db::{Db, query_all},
        flags::{self, CATALOG, Flags},
    },
};

pub async fn overview(
    db: &Db,
    actor: &Actor,
    effective: &Flags,
    db_path: &Path,
) -> Result<AdminOverview, AppError> {
    actor.require(Permission::AdminRead)?;
    let conn = db.read().await?;
    let stored = flags::stored(&conn).await?;
    let flags = CATALOG
        .iter()
        .map(|d| {
            let effective = effective.is_enabled(d.key);
            let stored = stored.get(d.key).copied();
            let implemented = flags::is_implemented(d.key);
            FlagView {
                key: d.key.into(),
                description: d.description.into(),
                effective,
                stored,
                implemented,
                pending_restart: pending_restart(effective, stored, implemented),
            }
        })
        .collect();
    Ok(AdminOverview {
        flags,
        stats: stats(&conn, effective, db_path).await?,
    })
}

async fn stats(
    conn: &Connection,
    effective: &Flags,
    db_path: &Path,
) -> Result<InstanceStats, AppError> {
    let mut users_by_status = Vec::new();
    for row in query_all(
        conn,
        "SELECT status, COUNT(*) FROM users GROUP BY status ORDER BY status",
        (),
    )
    .await?
    {
        users_by_status.push((row.get::<String>(0)?, row.get::<i64>(1)?));
    }
    let mut migrations = Vec::new();
    for row in query_all(
        conn,
        "SELECT version FROM schema_migrations ORDER BY version",
        (),
    )
    .await?
    {
        migrations.push(row.get::<String>(0)?);
    }
    let mut catalogs = Vec::new();
    for row in query_all(
        conn,
        "SELECT catalog, checksum, imported_at FROM catalog_versions ORDER BY catalog",
        (),
    )
    .await?
    {
        let checksum: String = row.get(1)?;
        catalogs.push((
            row.get::<String>(0)?,
            checksum.chars().take(12).collect(),
            row.get::<String>(2)?,
        ));
    }
    Ok(InstanceStats {
        users_by_status,
        db_bytes: db_bytes(db_path),
        migrations,
        catalogs,
        effective_flags: effective
            .enabled_keys()
            .into_iter()
            .map(String::from)
            .collect(),
    })
}

/// Main file plus write-ahead log, as found on disk.
fn db_bytes(path: &Path) -> u64 {
    let wal = format!("{}-wal", path.display());
    [path, Path::new(&wal)]
        .iter()
        .filter_map(|p| std::fs::metadata(p).ok())
        .map(|m| m.len())
        .sum()
}

pub async fn set_flag(
    db: &Db,
    now: DateTime<Utc>,
    actor: &Actor,
    key: &str,
    enabled: bool,
) -> Result<(), AppError> {
    actor.require(Permission::FlagsManage)?;
    let tx = db.write().await?;
    flags::store_in(&tx, now, key, enabled, Some(&actor.user_id)).await?;
    audit::append(
        &tx,
        now,
        Event {
            kind: "admin.flag_changed",
            visibility: Visibility::Restricted,
            actor_ref: Some(&actor.user_id),
            subject: Some(key),
            payload: json!({ "key": key, "enabled": enabled }),
        },
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        application::{
            audit::{ChainStatus, verify},
            session::{self, SessionPolicy, testing::account_with_session},
        },
        infrastructure::{
            catalog,
            db::{query_opt, testing::migrated},
            flags::VOTING_PRIVATE,
        },
    };

    const MINI: &str = "code,kind,name,parent_code,state_abbrev\nBR,national,Brasil,,\n42,state,SC,BR,SC\n4202404,municipal,Blumenau,42,SC\n";

    fn policy() -> SessionPolicy {
        SessionPolicy::from_secs(900, 28800)
    }

    fn now() -> DateTime<Utc> {
        "2026-10-24T12:00:00Z".parse().unwrap()
    }

    async fn actor(db: &Db, status: &str, policies: &[&str]) -> (Actor, String) {
        let (_, token) =
            account_with_session(db, now(), policy(), "4202404", status, policies).await;
        let actor = session::resolve(db, now(), policy(), &token, false)
            .await
            .unwrap()
            .unwrap();
        let email: String = query_opt(
            &db.read().await.unwrap(),
            "SELECT email FROM users WHERE id = ?1",
            [actor.user_id.as_str()],
        )
        .await
        .unwrap()
        .unwrap()
        .get(0)
        .unwrap();
        (actor, email)
    }

    async fn setup() -> crate::infrastructure::db::testing::TestDb {
        let t = migrated().await;
        catalog::import_str(&t.db, "mini", MINI).await.unwrap();
        t
    }

    #[tokio::test]
    async fn only_policies_granting_admin_actions_may_administer() {
        let t = setup().await;
        let path = t.dir.path().join("test.db");
        let flags = Flags::default();
        for (status, policies) in [
            ("active", &["Cidadao"][..]),
            ("active", &["Auditor", "Moderador"][..]),
            ("pending", &["Cidadao", "AdministratorAccess"][..]),
        ] {
            let (a, _) = actor(&t.db, status, policies).await;
            assert_eq!(
                overview(&t.db, &a, &flags, &path).await,
                Err(AppError::Forbidden)
            );
            assert_eq!(
                set_flag(&t.db, now(), &a, flags::PROPOSALS, true).await,
                Err(AppError::Forbidden)
            );
        }
        let (admin, _) = actor(&t.db, "active", &["AdministratorAccess"]).await;
        let o = overview(&t.db, &admin, &flags, &path).await.unwrap();
        assert_eq!(o.flags.len(), CATALOG.len());
        assert_eq!(
            o.stats.users_by_status,
            vec![("active".into(), 3), ("pending".into(), 1)]
        );
        assert!(o.stats.db_bytes > 0);
        assert!(o.stats.migrations.contains(&"0500_iam".into()));
    }

    /// A customer policy may grant reading the panel without changing flags.
    #[tokio::test]
    async fn read_only_admin_policy_cannot_change_flags() {
        let t = setup().await;
        let path = t.dir.path().join("test.db");
        let (reader, _) = actor(&t.db, "active", &["Cidadao"]).await;
        let tx = t.db.write().await.unwrap();
        tx.execute(
            "INSERT INTO iam_policies (id, name, document, created_at, updated_at) VALUES ('p-ro', 'LeituraAdmin',
             '{\"Version\":\"2026-10-25\",\"Statement\":[{\"Effect\":\"Allow\",\"Action\":\"admin:read\",\"Resource\":\"*\"}]}', 't', 't')",
            (),
        )
        .await
        .unwrap();
        session::testing::attach_policy(&tx, &reader.user_id, "LeituraAdmin").await;
        tx.commit().await.unwrap();
        let conn = t.db.read().await.unwrap();
        let reader = crate::application::actor::load(&conn, &reader.user_id, &reader.session_hash)
            .await
            .unwrap()
            .unwrap();
        assert!(
            overview(&t.db, &reader, &Flags::default(), &path)
                .await
                .is_ok()
        );
        assert_eq!(
            set_flag(&t.db, now(), &reader, flags::PROPOSALS, true).await,
            Err(AppError::Forbidden)
        );
    }

    #[tokio::test]
    async fn flag_change_is_audited_and_pending_until_restart() {
        let t = setup().await;
        let path = t.dir.path().join("test.db");
        let effective = Flags::load(&t.db).await.unwrap();
        let (admin, _) = actor(&t.db, "active", &["AdministratorAccess"]).await;
        set_flag(&t.db, now(), &admin, flags::PROPOSALS, true)
            .await
            .unwrap();
        let o = overview(&t.db, &admin, &effective, &path).await.unwrap();
        let f = o.flags.iter().find(|f| f.key == flags::PROPOSALS).unwrap();
        assert!(!f.effective && f.stored == Some(true) && f.pending_restart);
        assert!(!effective.is_enabled(flags::PROPOSALS));
        // After "restart" the stored value is effective and nothing is pending.
        let reloaded = Flags::load(&t.db).await.unwrap();
        let o = overview(&t.db, &admin, &reloaded, &path).await.unwrap();
        let f = o.flags.iter().find(|f| f.key == flags::PROPOSALS).unwrap();
        assert!(f.effective && !f.pending_restart);
        let conn = t.db.read().await.unwrap();
        let row = query_opt(
            &conn,
            "SELECT visibility, actor_ref, subject, payload FROM audit_events WHERE kind = 'admin.flag_changed'",
            (),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(row.get::<String>(0).unwrap(), "restricted");
        assert_eq!(row.get::<String>(1).unwrap(), admin.user_id);
        assert_eq!(row.get::<String>(2).unwrap(), flags::PROPOSALS);
        assert!(matches!(
            verify(&conn).await.unwrap(),
            ChainStatus::Valid { events: 1, .. }
        ));
    }

    #[tokio::test]
    async fn private_voting_cannot_be_enabled_and_leaves_no_trace() {
        let t = setup().await;
        let path = t.dir.path().join("test.db");
        let (admin, _) = actor(&t.db, "active", &["AdministratorAccess"]).await;
        assert!(matches!(
            set_flag(&t.db, now(), &admin, VOTING_PRIVATE, true).await,
            Err(AppError::Validation { .. })
        ));
        assert!(
            set_flag(&t.db, now(), &admin, "unknown.flag", true)
                .await
                .is_err()
        );
        let o = overview(&t.db, &admin, &Flags::default(), &path)
            .await
            .unwrap();
        let f = o.flags.iter().find(|f| f.key == VOTING_PRIVATE).unwrap();
        assert!(!f.implemented && f.stored.is_none() && !f.pending_restart);
        let conn = t.db.read().await.unwrap();
        assert!(matches!(
            verify(&conn).await.unwrap(),
            ChainStatus::Valid { events: 0, .. }
        ));
    }
}
