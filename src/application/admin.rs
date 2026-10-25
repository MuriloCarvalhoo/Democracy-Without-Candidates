//! Admin País use cases: feature flags, auditor accreditation and instance health.
//! Every operation checks the permission here; the panel only reflects the result.
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
        admin::{AdminOverview, AuditorView, FlagView, InstanceStats, pending_restart},
        identity::{Permission, Role, normalize_email},
    },
    error::AppError,
    infrastructure::{
        clock::ts,
        db::{Db, query_all, query_opt},
        flags::{self, CATALOG, Flags},
    },
};

/// Same message whether the account is missing, inactive or the email is invalid.
const ACCOUNT_NOT_ELIGIBLE: &str = "Conta não encontrada ou não elegível.";

#[tracing::instrument(level = "debug", skip_all, fields(actor = %actor.user_id), err(level = "debug"))]
pub async fn overview(
    db: &Db,
    actor: &Actor,
    effective: &Flags,
    db_path: &Path,
) -> Result<AdminOverview, AppError> {
    actor.require(Permission::FlagsManage)?;
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

#[tracing::instrument(level = "debug", skip_all, fields(actor = %actor.user_id, key = %key, enabled = %enabled), err(level = "debug"))]
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

#[tracing::instrument(level = "debug", skip_all, fields(actor = %actor.user_id), err(level = "debug"))]
pub async fn list_auditors(db: &Db, actor: &Actor) -> Result<Vec<AuditorView>, AppError> {
    actor.require(Permission::AuditorsManage)?;
    let conn = db.read().await?;
    let mut out = Vec::new();
    for row in query_all(
        &conn,
        "SELECT u.email, u.name, r.granted_at FROM user_roles r JOIN users u ON u.id = r.user_id
         WHERE r.role = 'AUDITOR' ORDER BY r.granted_at",
        (),
    )
    .await?
    {
        out.push(AuditorView {
            email: row.get(0)?,
            name: row.get(1)?,
            granted_at: row.get(2)?,
        });
    }
    Ok(out)
}

#[tracing::instrument(level = "debug", skip_all, fields(actor = %actor.user_id), err(level = "debug"))]
pub async fn grant_auditor(
    db: &Db,
    now: DateTime<Utc>,
    actor: &Actor,
    email: &str,
) -> Result<(), AppError> {
    actor.require(Permission::AuditorsManage)?;
    grant_role(db, now, email, Role::Auditor, &actor.user_id).await
}

#[tracing::instrument(level = "debug", skip_all, fields(actor = %actor.user_id), err(level = "debug"))]
pub async fn revoke_auditor(
    db: &Db,
    now: DateTime<Utc>,
    actor: &Actor,
    email: &str,
) -> Result<(), AppError> {
    actor.require(Permission::AuditorsManage)?;
    let email = normalize_email(email).ok_or_else(not_eligible)?;
    let tx = db.write().await?;
    let user_id = account_id(&tx, &email, false).await?;
    let removed = tx
        .execute(
            "DELETE FROM user_roles WHERE user_id = ?1 AND role = 'AUDITOR'",
            [user_id.as_str()],
        )
        .await?;
    if removed == 0 {
        return Err(not_eligible());
    }
    audit::append(
        &tx,
        now,
        Event {
            kind: "admin.auditor_revoked",
            visibility: Visibility::Restricted,
            actor_ref: Some(&actor.user_id),
            subject: Some(&user_id),
            payload: json!({ "role": Role::Auditor.as_db() }),
        },
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

/// Operator bootstrap from the VPS shell (`dwc grant-admin <email>`); not exposed
/// over HTTP. The account must already exist and be active.
#[tracing::instrument(level = "debug", skip_all, err(level = "debug"))]
pub async fn grant_country_admin(db: &Db, now: DateTime<Utc>, email: &str) -> Result<(), AppError> {
    grant_role(db, now, email, Role::CountryAdmin, "operator:cli").await
}

async fn grant_role(
    db: &Db,
    now: DateTime<Utc>,
    email: &str,
    role: Role,
    by: &str,
) -> Result<(), AppError> {
    let email = normalize_email(email).ok_or_else(not_eligible)?;
    let tx = db.write().await?;
    let user_id = account_id(&tx, &email, true).await?;
    let added = tx
        .execute(
            "INSERT INTO user_roles (user_id, role, granted_at, granted_by) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(user_id, role) DO NOTHING",
            (user_id.as_str(), role.as_db(), ts(now), by),
        )
        .await?;
    if added == 0 {
        return Err(AppError::Conflict("A conta já possui este papel.".into()));
    }
    let kind = match role {
        Role::Auditor => "admin.auditor_granted",
        _ => "admin.role_granted",
    };
    audit::append(
        &tx,
        now,
        Event {
            kind,
            visibility: Visibility::Restricted,
            actor_ref: Some(by),
            subject: Some(&user_id),
            payload: json!({ "role": role.as_db() }),
        },
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

async fn account_id(conn: &Connection, email: &str, active_only: bool) -> Result<String, AppError> {
    let row = query_opt(
        conn,
        "SELECT id, status FROM users WHERE email = ?1",
        [email],
    )
    .await?
    .ok_or_else(not_eligible)?;
    if active_only && row.get::<String>(1)? != "active" {
        return Err(not_eligible());
    }
    Ok(row.get(0)?)
}

fn not_eligible() -> AppError {
    AppError::validation("email", ACCOUNT_NOT_ELIGIBLE)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        application::{
            audit::{ChainStatus, verify},
            session::{self, SessionPolicy, testing::account_with_session},
        },
        infrastructure::{catalog, db::testing::migrated, flags::VOTING_PRIVATE},
    };

    const MINI: &str = "code,kind,name,parent_code,state_abbrev\nBR,national,Brasil,,\n42,state,SC,BR,SC\n4202404,municipal,Blumenau,42,SC\n";

    fn policy() -> SessionPolicy {
        SessionPolicy::from_secs(900, 28800)
    }

    fn now() -> DateTime<Utc> {
        "2026-10-24T12:00:00Z".parse().unwrap()
    }

    async fn actor(db: &Db, status: &str, roles: &[&str]) -> (Actor, String) {
        let (_, token) = account_with_session(db, now(), policy(), "4202404", status, roles).await;
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
    async fn only_active_country_admin_may_administer() {
        let t = setup().await;
        let path = t.dir.path().join("test.db");
        let flags = Flags::default();
        let (target, target_email) = actor(&t.db, "active", &["CITIZEN"]).await;
        for (status, roles) in [
            ("active", &["CITIZEN"][..]),
            ("active", &["AUDITOR"][..]),
            ("pending", &["CITIZEN", "COUNTRY_ADMIN"][..]),
        ] {
            let (a, _) = actor(&t.db, status, roles).await;
            assert_eq!(
                overview(&t.db, &a, &flags, &path).await,
                Err(AppError::Forbidden)
            );
            assert_eq!(
                set_flag(&t.db, now(), &a, flags::PROPOSALS, true).await,
                Err(AppError::Forbidden)
            );
            assert_eq!(list_auditors(&t.db, &a).await, Err(AppError::Forbidden));
            assert_eq!(
                grant_auditor(&t.db, now(), &a, &target_email).await,
                Err(AppError::Forbidden)
            );
        }
        let (admin, _) = actor(&t.db, "active", &["COUNTRY_ADMIN"]).await;
        let o = overview(&t.db, &admin, &flags, &path).await.unwrap();
        assert_eq!(o.flags.len(), CATALOG.len());
        assert_eq!(
            o.stats.users_by_status,
            vec![("active".into(), 4), ("pending".into(), 1)]
        );
        assert!(o.stats.db_bytes > 0);
        assert!(o.stats.migrations.contains(&"0001_foundation".into()));
        assert!(!target.has_role(Role::Auditor));
    }

    #[tokio::test]
    async fn flag_change_is_audited_and_pending_until_restart() {
        let t = setup().await;
        let path = t.dir.path().join("test.db");
        let effective = Flags::load(&t.db).await.unwrap();
        let (admin, _) = actor(&t.db, "active", &["COUNTRY_ADMIN"]).await;
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
        let (admin, _) = actor(&t.db, "active", &["COUNTRY_ADMIN"]).await;
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

    #[tokio::test]
    async fn auditor_grant_and_revoke() {
        let t = setup().await;
        let (admin, _) = actor(&t.db, "active", &["COUNTRY_ADMIN"]).await;
        let (_, email) = actor(&t.db, "active", &["CITIZEN"]).await;
        let (_, pending_email) = actor(&t.db, "pending", &["CITIZEN"]).await;
        let generic = Err(not_eligible());
        assert_eq!(
            grant_auditor(&t.db, now(), &admin, "nobody@example.test").await,
            generic
        );
        assert_eq!(
            grant_auditor(&t.db, now(), &admin, &pending_email).await,
            generic
        );
        assert_eq!(
            grant_auditor(&t.db, now(), &admin, "not-an-email").await,
            generic
        );
        grant_auditor(
            &t.db,
            now(),
            &admin,
            &format!("  {}  ", email.to_uppercase()),
        )
        .await
        .unwrap();
        assert!(matches!(
            grant_auditor(&t.db, now(), &admin, &email).await,
            Err(AppError::Conflict(_))
        ));
        let list = list_auditors(&t.db, &admin).await.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].email, email);
        revoke_auditor(&t.db, now(), &admin, &email).await.unwrap();
        assert_eq!(revoke_auditor(&t.db, now(), &admin, &email).await, generic);
        assert!(list_auditors(&t.db, &admin).await.unwrap().is_empty());
        let conn = t.db.read().await.unwrap();
        assert!(matches!(
            verify(&conn).await.unwrap(),
            ChainStatus::Valid { events: 2, .. }
        ));
    }

    #[tokio::test]
    async fn cli_bootstrap_requires_existing_active_account() {
        let t = setup().await;
        let (citizen, email) = actor(&t.db, "active", &["CITIZEN"]).await;
        let (_, pending_email) = actor(&t.db, "pending", &["CITIZEN"]).await;
        assert!(
            grant_country_admin(&t.db, now(), "missing@example.test")
                .await
                .is_err()
        );
        assert!(
            grant_country_admin(&t.db, now(), &pending_email)
                .await
                .is_err()
        );
        grant_country_admin(&t.db, now(), &email).await.unwrap();
        let conn = t.db.read().await.unwrap();
        let reloaded = crate::application::actor::load(&conn, &citizen.user_id, "x")
            .await
            .unwrap()
            .unwrap();
        assert!(reloaded.can(Permission::FlagsManage));
        let row = query_opt(
            &conn,
            "SELECT actor_ref FROM audit_events WHERE kind = 'admin.role_granted'",
            (),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(row.get::<String>(0).unwrap(), "operator:cli");
    }
}
