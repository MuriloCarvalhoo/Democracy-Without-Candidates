//! Authentication failure limits: 5 failures per account and 30 per client IP in a
//! sliding 15-minute window. No permanent automatic block. Keys are hashes; IPs are
//! never stored in clear or written to audit events.
//!
//! Limitation: the IP comes from the socket peer. Behind a reverse proxy every
//! client shares the proxy address, so loopback peers are not IP-limited (the
//! account limit still applies) until a trusted forwarded-for setting exists.

use std::net::IpAddr;

use chrono::{DateTime, Duration, Utc};
use turso::Connection;

use crate::{
    error::AppError,
    infrastructure::{
        clock::ts,
        crypto::{new_id, sha256_hex},
        db::query_opt,
    },
};

pub const WINDOW: Duration = Duration::minutes(15);
pub const ACCOUNT_LIMIT: i64 = 5;
pub const IP_LIMIT: i64 = 30;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientKeys {
    pub account: String,
    pub ip: Option<String>,
}

impl ClientKeys {
    /// `email` must already be normalized (or the raw input when invalid).
    pub fn new(email: &str, ip: Option<IpAddr>) -> Self {
        Self {
            account: account_key(email),
            ip: ip
                .filter(|ip| !ip.is_loopback())
                .map(|ip| sha256_hex(format!("ip:{ip}"))),
        }
    }
}

pub fn account_key(email: &str) -> String {
    sha256_hex(format!("account:{email}"))
}

async fn count(
    conn: &Connection,
    now: DateTime<Utc>,
    scope: &str,
    key: &str,
) -> Result<i64, AppError> {
    Ok(query_opt(
        conn,
        "SELECT COUNT(*) FROM auth_failures WHERE scope = ?1 AND key = ?2 AND occurred_at > ?3",
        (scope, key, ts(now - WINDOW)),
    )
    .await?
    .map(|r| r.get::<i64>(0))
    .transpose()?
    .unwrap_or(0))
}

pub async fn check(
    conn: &Connection,
    now: DateTime<Utc>,
    keys: &ClientKeys,
) -> Result<(), AppError> {
    if count(conn, now, "account", &keys.account).await? >= ACCOUNT_LIMIT {
        return Err(AppError::RateLimited);
    }
    if let Some(ip) = &keys.ip
        && count(conn, now, "ip", ip).await? >= IP_LIMIT
    {
        return Err(AppError::RateLimited);
    }
    Ok(())
}

pub async fn record_failure(
    conn: &Connection,
    now: DateTime<Utc>,
    keys: &ClientKeys,
) -> Result<(), AppError> {
    conn.execute(
        "DELETE FROM auth_failures WHERE occurred_at <= ?1",
        [ts(now - WINDOW)],
    )
    .await?;
    let mut entries = vec![("account", keys.account.as_str())];
    if let Some(ip) = &keys.ip {
        entries.push(("ip", ip.as_str()));
    }
    for (scope, key) in entries {
        conn.execute(
            "INSERT INTO auth_failures (id, scope, key, occurred_at) VALUES (?1, ?2, ?3, ?4)",
            (new_id(), scope, key, ts(now)),
        )
        .await?;
    }
    Ok(())
}

/// Clears account failures after a complete successful authentication.
pub async fn clear_account(conn: &Connection, keys: &ClientKeys) -> Result<(), AppError> {
    conn.execute(
        "DELETE FROM auth_failures WHERE scope = 'account' AND key = ?1",
        [keys.account.as_str()],
    )
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::db::testing::migrated;

    #[tokio::test]
    async fn account_and_ip_windows() {
        let t = migrated().await;
        let now: DateTime<Utc> = "2026-10-24T12:00:00Z".parse().unwrap();
        let ip: IpAddr = "203.0.113.7".parse().unwrap();
        let keys = ClientKeys::new("ana@example.org", Some(ip));
        let conn = t.db.read().await.unwrap();
        for i in 0..ACCOUNT_LIMIT {
            check(&conn, now, &keys).await.unwrap();
            let tx = t.db.write().await.unwrap();
            record_failure(&tx, now + Duration::seconds(i), &keys)
                .await
                .unwrap();
            tx.commit().await.unwrap();
        }
        assert_eq!(check(&conn, now, &keys).await, Err(AppError::RateLimited));
        let other_account = ClientKeys::new("bia@example.org", None);
        assert!(
            check(&conn, now, &other_account).await.is_ok(),
            "per account"
        );
        // Sliding window: no permanent block.
        assert!(
            check(&conn, now + WINDOW + Duration::seconds(5), &keys)
                .await
                .is_ok()
        );

        // Per-IP limit across many accounts.
        for i in 0..IP_LIMIT {
            let k = ClientKeys::new(&format!("u{i}@example.org"), Some(ip));
            let tx = t.db.write().await.unwrap();
            record_failure(&tx, now, &k).await.unwrap();
            tx.commit().await.unwrap();
        }
        let fresh = ClientKeys::new("nova@example.org", Some(ip));
        assert_eq!(check(&conn, now, &fresh).await, Err(AppError::RateLimited));
        assert!(
            ClientKeys::new("x@example.org", Some("127.0.0.1".parse().unwrap()))
                .ip
                .is_none(),
            "loopback (proxy/dev) is not IP-limited"
        );
        let stored: String = query_opt(
            &conn,
            "SELECT key FROM auth_failures WHERE scope = 'ip' LIMIT 1",
            (),
        )
        .await
        .unwrap()
        .unwrap()
        .get(0)
        .unwrap();
        assert!(!stored.contains("203.0.113.7"), "IP stored only as hash");
    }
}
