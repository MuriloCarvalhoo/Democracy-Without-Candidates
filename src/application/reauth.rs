//! Reauthentication grants for critical actions: bound to session + action +
//! resource, valid for 5 minutes and consumed once inside the action transaction.

use chrono::{DateTime, Duration, Utc};
use turso::Connection;

use crate::{
    error::AppError,
    infrastructure::{clock::ts, crypto::new_id},
};

pub const VALIDITY: Duration = Duration::minutes(5);

/// Records a grant after face + TOTP were verified by the identity use case.
pub async fn grant(
    conn: &Connection,
    now: DateTime<Utc>,
    session_hash: &str,
    action: &str,
    resource: &str,
) -> Result<String, AppError> {
    let id = new_id();
    conn.execute(
        "INSERT INTO reauth_grants (id, session_hash, action, resource, created_at, expires_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        (id.as_str(), session_hash, action, resource, ts(now), ts(now + VALIDITY)),
    )
    .await?;
    Ok(id)
}

/// Consumes one valid grant or fails with `ReauthRequired`. Call inside the same
/// write transaction as the protected effect so a rollback restores the grant.
pub async fn consume(
    conn: &Connection,
    now: DateTime<Utc>,
    session_hash: &str,
    action: &str,
    resource: &str,
) -> Result<(), AppError> {
    let changed = conn
        .execute(
            "UPDATE reauth_grants SET consumed_at = ?4 WHERE id = (
                SELECT id FROM reauth_grants
                WHERE session_hash = ?1 AND action = ?2 AND resource = ?3 AND consumed_at IS NULL AND expires_at > ?4
                ORDER BY created_at LIMIT 1)",
            (session_hash, action, resource, ts(now)),
        )
        .await?;
    if changed == 1 {
        Ok(())
    } else {
        Err(AppError::ReauthRequired)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        application::session::{SessionPolicy, testing::citizen_with_session, token_hash},
        infrastructure::{catalog, db::testing::migrated},
    };

    #[tokio::test]
    async fn grant_is_single_use_bound_and_expires() {
        let t = migrated().await;
        catalog::import_str(&t.db, "mini", "code,kind,name,parent_code,state_abbrev\nBR,national,Brasil,,\n42,state,SC,BR,SC\n4202404,municipal,Blumenau,42,SC\n").await.unwrap();
        let now: DateTime<Utc> = "2026-10-24T12:00:00Z".parse().unwrap();
        let (_, token) =
            citizen_with_session(&t.db, now, SessionPolicy::from_secs(900, 28800), "4202404").await;
        let s = token_hash(&token);
        let tx = t.db.write().await.unwrap();
        grant(&tx, now, &s, "proposal:sign", "p1").await.unwrap();
        assert_eq!(
            consume(&tx, now, &s, "proposal:vote", "p1").await,
            Err(AppError::ReauthRequired),
            "other action"
        );
        assert_eq!(
            consume(&tx, now, &s, "proposal:sign", "p2").await,
            Err(AppError::ReauthRequired),
            "other resource"
        );
        assert_eq!(
            consume(&tx, now, "other-session", "proposal:sign", "p1").await,
            Err(AppError::ReauthRequired)
        );
        assert_eq!(
            consume(&tx, now + VALIDITY, &s, "proposal:sign", "p1").await,
            Err(AppError::ReauthRequired),
            "expired"
        );
        assert_eq!(
            consume(&tx, now + Duration::minutes(4), &s, "proposal:sign", "p1").await,
            Ok(())
        );
        assert_eq!(
            consume(&tx, now + Duration::minutes(4), &s, "proposal:sign", "p1").await,
            Err(AppError::ReauthRequired),
            "replay"
        );
        tx.commit().await.unwrap();
    }
}
