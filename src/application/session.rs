//! Server-side sessions in TursoDB. The cookie carries a random 32-byte token; only
//! its SHA-256 is stored. Idle (15 min) and absolute (8 h) limits survive restarts.

use chrono::{DateTime, Duration, Utc};

use super::actor::{self, Actor};
use crate::{
    error::AppError,
    infrastructure::{
        clock::{parse_ts, ts},
        crypto::{random_token, sha256_hex},
        db::{Db, query_opt},
    },
};

#[derive(Debug, Clone, Copy)]
pub struct SessionPolicy {
    pub idle: Duration,
    pub absolute: Duration,
}

impl SessionPolicy {
    pub fn from_secs(idle: i64, absolute: i64) -> Self {
        Self {
            idle: Duration::seconds(idle),
            absolute: Duration::seconds(absolute),
        }
    }
}

pub fn token_hash(token: &str) -> String {
    sha256_hex(token)
}

/// Creates a session inside the caller's transaction; returns the cookie token.
pub async fn create(
    conn: &turso::Connection,
    now: DateTime<Utc>,
    policy: SessionPolicy,
    user_id: &str,
) -> Result<String, AppError> {
    let token = random_token(32);
    let absolute = now + policy.absolute;
    let idle = (now + policy.idle).min(absolute);
    conn.execute(
        "INSERT INTO sessions (id_hash, user_id, created_at, last_seen_at, idle_expires_at, absolute_expires_at)
         VALUES (?1, ?2, ?3, ?3, ?4, ?5)",
        (token_hash(&token), user_id, ts(now), ts(idle), ts(absolute)),
    )
    .await?;
    Ok(token)
}

/// Resolves the actor for a cookie token. `touch` renews idle expiry (explicit
/// navigation/actions); background polling must pass `false`.
/// Expired or revoked sessions return `SessionExpired`; unknown tokens `Ok(None)`.
pub async fn resolve(
    db: &Db,
    now: DateTime<Utc>,
    policy: SessionPolicy,
    token: &str,
    touch: bool,
) -> Result<Option<Actor>, AppError> {
    let hash = token_hash(token);
    let conn = db.read().await?;
    let Some(row) = query_opt(
        &conn,
        "SELECT user_id, idle_expires_at, absolute_expires_at, revoked_at FROM sessions WHERE id_hash = ?1",
        [hash.as_str()],
    )
    .await?
    else {
        return Ok(None);
    };
    let user_id: String = row.get(0)?;
    let idle = parse_ts(&row.get::<String>(1)?).ok_or(AppError::Internal)?;
    let absolute = parse_ts(&row.get::<String>(2)?).ok_or(AppError::Internal)?;
    let revoked: Option<String> = row.get(3)?;
    if revoked.is_some() || now >= idle || now >= absolute {
        return Err(AppError::SessionExpired);
    }
    let Some(actor) = actor::load(&conn, &user_id, &hash).await? else {
        return Ok(None);
    };
    if actor.status == crate::domain::identity::AccountStatus::Blocked {
        return Err(AppError::Forbidden);
    }
    if touch {
        let tx = db.write().await?;
        let new_idle = (now + policy.idle).min(absolute);
        tx.execute(
            "UPDATE sessions SET last_seen_at = ?2, idle_expires_at = ?3
             WHERE id_hash = ?1 AND revoked_at IS NULL AND idle_expires_at > ?2",
            (hash.as_str(), ts(now), ts(new_idle)),
        )
        .await?;
        tx.commit().await?;
    }
    Ok(Some(actor))
}

pub async fn revoke(
    conn: &turso::Connection,
    now: DateTime<Utc>,
    session_hash: &str,
    reason: &str,
) -> Result<(), AppError> {
    conn.execute(
        "UPDATE sessions SET revoked_at = ?2, revoke_reason = ?3 WHERE id_hash = ?1 AND revoked_at IS NULL",
        (session_hash, ts(now), reason),
    )
    .await?;
    Ok(())
}

/// Revokes every active session of a user (password change/reset, blocking).
pub async fn revoke_all(
    conn: &turso::Connection,
    now: DateTime<Utc>,
    user_id: &str,
    reason: &str,
) -> Result<u64, AppError> {
    Ok(conn
        .execute(
            "UPDATE sessions SET revoked_at = ?2, revoke_reason = ?3 WHERE user_id = ?1 AND revoked_at IS NULL",
            (user_id, ts(now), reason),
        )
        .await?)
}

#[cfg(test)]
pub mod testing {
    use super::*;
    use crate::infrastructure::crypto::new_id;

    /// Inserts an active synthetic citizen in the first municipality of the
    /// catalog fixture and returns (user_id, session token). Test-only.
    pub async fn citizen_with_session(
        db: &Db,
        now: DateTime<Utc>,
        policy: SessionPolicy,
        municipality_code: &str,
    ) -> (String, String) {
        account_with_session(db, now, policy, municipality_code, "active", &["CITIZEN"]).await
    }

    /// Like `citizen_with_session` with explicit status (`pending`/`active`/`blocked`)
    /// and roles (`CITIZEN`, `AUDITOR`, `COUNTRY_ADMIN`). Test-only.
    pub async fn account_with_session(
        db: &Db,
        now: DateTime<Utc>,
        policy: SessionPolicy,
        municipality_code: &str,
        status: &str,
        roles: &[&str],
    ) -> (String, String) {
        let user_id = new_id();
        let tx = db.write().await.unwrap();
        let row = query_opt(
            &tx,
            "SELECT m.id, s.id, s.parent_id FROM jurisdictions m JOIN jurisdictions s ON s.id = m.parent_id WHERE m.code = ?1",
            [municipality_code],
        )
        .await
        .unwrap()
        .expect("municipality in catalog");
        let (m, s, c): (String, String, String) = (
            row.get(0).unwrap(),
            row.get(1).unwrap(),
            row.get(2).unwrap(),
        );
        tx.execute(
            "INSERT INTO users (id, email, name, password_hash, status, municipality_id, state_id, country_id, synthetic, created_at, updated_at)
             VALUES (?1, ?2, 'Cidadã Sintética', 'x', ?7, ?3, ?4, ?5, 1, ?6, ?6)",
            (user_id.as_str(), format!("{user_id}@example.test"), m, s, c, ts(now), status),
        )
        .await
        .unwrap();
        for role in roles {
            tx.execute(
                "INSERT INTO user_roles (user_id, role, granted_at) VALUES (?1, ?2, ?3)",
                (user_id.as_str(), *role, ts(now)),
            )
            .await
            .unwrap();
        }
        let token = create(&tx, now, policy, &user_id).await.unwrap();
        tx.commit().await.unwrap();
        (user_id, token)
    }
}

#[cfg(test)]
mod tests {
    use super::{testing::citizen_with_session, *};
    use crate::infrastructure::{catalog, db::testing::migrated};

    const MINI_CATALOG: &str = "code,kind,name,parent_code,state_abbrev\nBR,national,Brasil,,\n42,state,Santa Catarina,BR,SC\n4202404,municipal,Blumenau,42,SC\n";

    async fn setup() -> (
        crate::infrastructure::db::testing::TestDb,
        SessionPolicy,
        DateTime<Utc>,
    ) {
        let t = migrated().await;
        catalog::import_str(&t.db, "mini", MINI_CATALOG)
            .await
            .unwrap();
        (
            t,
            SessionPolicy::from_secs(900, 8 * 3600),
            "2026-10-24T12:00:00Z".parse().unwrap(),
        )
    }

    #[tokio::test]
    async fn idle_expiry_boundary_and_renewal() {
        let (t, policy, now) = setup().await;
        let (_, token) = citizen_with_session(&t.db, now, policy, "4202404").await;
        let before = now + Duration::seconds(899);
        assert!(
            resolve(&t.db, before, policy, &token, false)
                .await
                .unwrap()
                .is_some()
        );
        assert_eq!(
            resolve(&t.db, now + Duration::seconds(900), policy, &token, false).await,
            Err(AppError::SessionExpired)
        );
        // Renewal by explicit activity extends the idle window.
        assert!(
            resolve(&t.db, before, policy, &token, true)
                .await
                .unwrap()
                .is_some()
        );
        assert!(
            resolve(
                &t.db,
                before + Duration::seconds(899),
                policy,
                &token,
                false
            )
            .await
            .unwrap()
            .is_some()
        );
    }

    #[tokio::test]
    async fn absolute_limit_caps_renewal() {
        let (t, policy, now) = setup().await;
        let (_, token) = citizen_with_session(&t.db, now, policy, "4202404").await;
        let mut at = now;
        while at < now + Duration::hours(8) - Duration::minutes(10) {
            at += Duration::minutes(10);
            assert!(
                resolve(&t.db, at, policy, &token, true)
                    .await
                    .unwrap()
                    .is_some()
            );
        }
        assert_eq!(
            resolve(&t.db, now + Duration::hours(8), policy, &token, true).await,
            Err(AppError::SessionExpired)
        );
    }

    #[tokio::test]
    async fn revocation_and_expiry_survive_reopen() {
        let (t, policy, now) = setup().await;
        let (user, token) = citizen_with_session(&t.db, now, policy, "4202404").await;
        let tx = t.db.write().await.unwrap();
        assert_eq!(
            revoke_all(&tx, now, &user, "password_reset").await.unwrap(),
            1
        );
        tx.commit().await.unwrap();
        let path = t.dir.path().join("test.db");
        drop(t.db);
        let reopened = Db::open(&path).await.unwrap();
        assert_eq!(
            resolve(&reopened, now, policy, &token, false).await,
            Err(AppError::SessionExpired)
        );
        assert_eq!(
            resolve(&reopened, now, policy, "unknown", false).await,
            Ok(None)
        );
    }

    #[tokio::test]
    async fn only_token_hash_is_stored() {
        let (t, policy, now) = setup().await;
        let (_, token) = citizen_with_session(&t.db, now, policy, "4202404").await;
        let conn = t.db.read().await.unwrap();
        assert!(
            query_opt(
                &conn,
                "SELECT 1 FROM sessions WHERE id_hash = ?1",
                [token.as_str()]
            )
            .await
            .unwrap()
            .is_none()
        );
        assert_eq!(token.len(), 64);
    }
}

#[cfg(test)]
mod identity_core_invariants {
    use super::{testing::citizen_with_session, *};
    use crate::infrastructure::{catalog, db::testing::migrated};

    #[tokio::test]
    async fn address_and_territory_are_immutable_in_the_database() {
        let t = migrated().await;
        catalog::import_str(&t.db, "mini", "code,kind,name,parent_code,state_abbrev\nBR,national,Brasil,,\n42,state,SC,BR,SC\n4202404,municipal,Blumenau,42,SC\n4205407,municipal,Florianópolis,42,SC\n").await.unwrap();
        let now = chrono::Utc::now();
        let (user, _) =
            citizen_with_session(&t.db, now, SessionPolicy::from_secs(900, 28800), "4202404").await;
        let conn = t.db.read().await.unwrap();
        conn.execute(
            "INSERT INTO addresses (user_id, cep, state, city, neighborhood, street, service, ibge, created_at)
             VALUES (?1, '89010025', 'SC', 'Blumenau', 'Centro', 'Rua Doutor Luiz de Freitas Melro', 'open-cep', '{\"city\":\"4202404\",\"state\":\"42\"}', 'now')",
            [user.as_str()],
        )
        .await
        .unwrap();
        assert!(
            conn.execute(
                "UPDATE addresses SET city = 'Outra' WHERE user_id = ?1",
                [user.as_str()]
            )
            .await
            .is_err()
        );
        assert!(
            conn.execute("DELETE FROM addresses WHERE user_id = ?1", [user.as_str()])
                .await
                .is_err()
        );
        let other = query_opt(
            &conn,
            "SELECT id FROM jurisdictions WHERE code = '4205407'",
            (),
        )
        .await
        .unwrap()
        .unwrap()
        .get::<String>(0)
        .unwrap();
        assert!(
            conn.execute(
                "UPDATE users SET municipality_id = ?2 WHERE id = ?1",
                (user.as_str(), other.as_str())
            )
            .await
            .is_err()
        );
        assert!(
            conn.execute(
                "UPDATE users SET name = 'Novo Nome' WHERE id = ?1",
                [user.as_str()]
            )
            .await
            .is_ok(),
            "other columns stay editable"
        );
    }
}
