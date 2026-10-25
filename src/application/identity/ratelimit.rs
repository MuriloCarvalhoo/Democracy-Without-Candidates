//! Authentication failure limits: 5 failures per account and 30 per client IP in a
//! sliding 15-minute window. No permanent automatic block. Keys are hashes; IPs are
//! never stored in clear or written to audit events.
//!
//! Pre-authentication flows (login/reset by e-mail) count per e-mail and IP.
//! Factor checks of a signed-in session count per user id only, so anonymous
//! failures for an e-mail never block the owner's reauthentication. Attempts are
//! reserved before verification (check + insert in one write transaction) and
//! released on success, so parallel bursts cannot exceed a limit.

use std::net::IpAddr;

use chrono::{DateTime, Duration, Utc};
use turso::Connection;

use crate::{
    error::AppError,
    infrastructure::{
        clock::ts,
        crypto::{new_id, sha256_hex},
        db::{Db, query_opt},
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
    /// Pre-authentication keys. `email` must already be normalized (or the raw input
    /// when invalid); `ip` comes from [`client_ip`] (`None`: not IP-limited).
    pub fn new(email: &str, ip: Option<IpAddr>) -> Self {
        Self {
            account: account_key(email),
            ip: ip.map(|ip| sha256_hex(format!("ip:{ip}"))),
        }
    }

    /// Factor checks of an authenticated session, bound to the user id.
    pub fn session(user_id: &str) -> Self {
        Self {
            account: sha256_hex(format!("user:{user_id}")),
            ip: None,
        }
    }
}

pub fn account_key(email: &str) -> String {
    sha256_hex(format!("account:{email}"))
}

/// Address used for the per-IP limit (`None`: not IP-limited).
/// Without trusted proxies: the socket peer, except loopback (local development).
/// Peer is a trusted proxy: the right-most `X-Forwarded-For` entry that is not a
/// trusted proxy (entries to its left are client-controlled); without a valid
/// entry the request is not IP-limited and the proxy never counts as a client.
/// Any other peer is used as-is and the header is ignored.
pub fn client_ip<'a>(
    peer: IpAddr,
    trusted: &[IpAddr],
    forwarded_for: impl IntoIterator<Item = &'a str>,
) -> Option<IpAddr> {
    let peer = peer.to_canonical();
    if trusted.is_empty() {
        return (!peer.is_loopback()).then_some(peer);
    }
    if !trusted.contains(&peer) {
        return Some(peer);
    }
    let entries: Vec<&str> = forwarded_for
        .into_iter()
        .flat_map(|v| v.split(','))
        .collect();
    for entry in entries.into_iter().rev() {
        let ip = entry.trim().parse::<IpAddr>().ok()?.to_canonical();
        if !trusted.contains(&ip) {
            return Some(ip);
        }
    }
    None
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

#[tracing::instrument(level = "debug", skip_all, err(level = "debug"))]
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

/// Failure rows recorded for one attempt.
#[must_use]
pub struct Attempt(Vec<String>);

#[tracing::instrument(level = "debug", skip_all, err(level = "debug"))]
pub async fn record_failure(
    conn: &Connection,
    now: DateTime<Utc>,
    keys: &ClientKeys,
) -> Result<Attempt, AppError> {
    conn.execute(
        "DELETE FROM auth_failures WHERE occurred_at <= ?1",
        [ts(now - WINDOW)],
    )
    .await?;
    let mut entries = vec![("account", keys.account.as_str())];
    if let Some(ip) = &keys.ip {
        entries.push(("ip", ip.as_str()));
    }
    let mut ids = Vec::with_capacity(entries.len());
    for (scope, key) in entries {
        let id = new_id();
        conn.execute(
            "INSERT INTO auth_failures (id, scope, key, occurred_at) VALUES (?1, ?2, ?3, ?4)",
            (id.as_str(), scope, key, ts(now)),
        )
        .await?;
        ids.push(id);
    }
    Ok(Attempt(ids))
}

/// Checks the limits and records the attempt as a failure in one write
/// transaction, before the factor is verified.
#[tracing::instrument(level = "debug", skip_all, err(level = "debug"))]
pub async fn reserve(db: &Db, now: DateTime<Utc>, keys: &ClientKeys) -> Result<Attempt, AppError> {
    let tx = db.write().await?;
    check(&tx, now, keys).await?;
    let attempt = record_failure(&tx, now, keys).await?;
    tx.commit().await?;
    Ok(attempt)
}

/// Removes a reserved attempt (verification succeeded or could not run).
#[tracing::instrument(level = "debug", skip_all, err(level = "debug"))]
pub async fn release(conn: &Connection, attempt: Attempt) -> Result<(), AppError> {
    for id in attempt.0 {
        conn.execute("DELETE FROM auth_failures WHERE id = ?1", [id])
            .await?;
    }
    Ok(())
}

/// Clears account failures after a complete successful authentication.
#[tracing::instrument(level = "debug", skip_all, err(level = "debug"))]
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
            let _ = record_failure(&tx, now + Duration::seconds(i), &keys)
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
            let _ = record_failure(&tx, now, &k).await.unwrap();
            tx.commit().await.unwrap();
        }
        let fresh = ClientKeys::new("nova@example.org", Some(ip));
        assert_eq!(check(&conn, now, &fresh).await, Err(AppError::RateLimited));
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

    #[test]
    fn session_keys_never_share_pre_auth_counters() {
        let session = ClientKeys::session("u1");
        assert_ne!(session.account, account_key("u1"));
        assert_ne!(
            session.account,
            ClientKeys::new("ana@example.org", None).account
        );
        assert!(session.ip.is_none());
    }

    #[tokio::test]
    async fn reserve_counts_before_verification_and_release_undoes_it() {
        let t = migrated().await;
        let now: DateTime<Utc> = "2026-10-24T12:00:00Z".parse().unwrap();
        let keys = ClientKeys::new("ana@example.org", None);
        let ok = reserve(&t.db, now, &keys).await.unwrap();
        let tx = t.db.write().await.unwrap();
        release(&tx, ok).await.unwrap();
        tx.commit().await.unwrap();
        for _ in 0..ACCOUNT_LIMIT {
            let _ = reserve(&t.db, now, &keys).await.unwrap();
        }
        assert!(matches!(
            reserve(&t.db, now, &keys).await,
            Err(AppError::RateLimited)
        ));
    }

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    #[test]
    fn client_ip_without_proxy_config_keeps_peer_and_exempts_loopback() {
        assert_eq!(
            client_ip(ip("203.0.113.7"), &[], ["198.51.100.1"]),
            Some(ip("203.0.113.7"))
        );
        assert_eq!(client_ip(ip("127.0.0.1"), &[], ["198.51.100.1"]), None);
        assert_eq!(client_ip(ip("::1"), &[], []), None);
    }

    #[test]
    fn client_ip_behind_trusted_proxy_uses_right_most_untrusted_entry() {
        let proxy = [ip("127.0.0.1")];
        // Caddy appends the real client to whatever the client sent.
        assert_eq!(
            client_ip(ip("127.0.0.1"), &proxy, ["10.9.9.9, 203.0.113.7"]),
            Some(ip("203.0.113.7"))
        );
        assert_eq!(
            client_ip(
                ip("127.0.0.1"),
                &proxy,
                ["1.1.1.1", "203.0.113.7, 127.0.0.1"]
            ),
            Some(ip("203.0.113.7")),
            "multiple headers, trusted hops skipped"
        );
        assert_eq!(
            client_ip(ip("::ffff:127.0.0.1"), &proxy, [" 2001:db8::1 "]),
            Some(ip("2001:db8::1"))
        );
        // A loopback client behind the proxy is limited like any other client.
        let two = [ip("127.0.0.1"), ip("192.0.2.10")];
        assert_eq!(
            client_ip(ip("127.0.0.1"), &two, ["::1, 192.0.2.10"]),
            Some(ip("::1"))
        );
        // No valid header: not IP-limited, the proxy is never the client.
        assert_eq!(client_ip(ip("127.0.0.1"), &proxy, []), None);
        assert_eq!(client_ip(ip("127.0.0.1"), &proxy, ["127.0.0.1"]), None);
        assert_eq!(
            client_ip(ip("127.0.0.1"), &proxy, ["1.2.3.4, garbage"]),
            None
        );
    }

    #[test]
    fn client_ip_ignores_forwarded_for_from_untrusted_peers() {
        let proxy = [ip("127.0.0.1")];
        assert_eq!(
            client_ip(ip("203.0.113.7"), &proxy, ["198.51.100.1"]),
            Some(ip("203.0.113.7")),
            "spoofed header ignored"
        );
        assert_eq!(
            client_ip(ip("::1"), &proxy, ["198.51.100.1"]),
            Some(ip("::1")),
            "with a proxy configured, other loopback peers are limited"
        );
    }
}
