//! Append-only audit events with a local SHA-256 chain. Payloads must be minimal:
//! never passwords, secrets, biometrics, IPs or anything linking a private ballot.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use turso::{Connection, Value};

use super::actor::Actor;
use crate::{
    domain::{
        audit::{
            ChainSummary, EventView, PAGE_SIZE, RESTRICTED_ACCESS, forbidden_payload_key,
            valid_kind,
        },
        identity::Permission,
    },
    error::AppError,
    infrastructure::{
        clock::ts,
        crypto::sha256_hex,
        db::{Db, query_all, query_opt},
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
    pub payload: serde_json::Value,
}

/// First object key (at any depth) naming data that must not be audited.
fn has_forbidden_key(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::Object(map) => map.iter().find_map(|(k, v)| {
            if forbidden_payload_key(k) {
                Some(k.clone())
            } else {
                has_forbidden_key(v)
            }
        }),
        serde_json::Value::Array(items) => items.iter().find_map(has_forbidden_key),
        _ => None,
    }
}

/// Hash of the previous hash followed by the stored fields
/// `[seq, occurred_at, kind, visibility, actor_ref, subject, payload]`.
fn chain_hash(prev: &str, fields: serde_json::Value) -> String {
    sha256_hex(serde_json::json!([prev, fields]).to_string())
}

/// Appends inside the caller's write transaction so the event commits with the effect.
/// Payloads with keys naming secrets/biometrics/contact/IP/ballot data are refused.
#[tracing::instrument(level = "debug", skip_all, err(level = "debug"))]
pub async fn append(
    conn: &Connection,
    now: DateTime<Utc>,
    event: Event<'_>,
) -> Result<i64, AppError> {
    if let Some(key) = has_forbidden_key(&event.payload) {
        tracing::error!(kind = event.kind, %key, "refused audit payload with forbidden key");
        return Err(AppError::Internal);
    }
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

impl From<ChainStatus> for ChainSummary {
    fn from(status: ChainStatus) -> Self {
        match status {
            ChainStatus::Valid { events, head } => Self::Valid { events, head },
            ChainStatus::Broken { at_seq } => Self::Broken { at_seq },
        }
    }
}

/// Recomputes the whole chain. Detects edits/gaps in the local copy only.
#[tracing::instrument(level = "debug", skip_all, err(level = "debug"))]
pub async fn verify(conn: &Connection) -> Result<ChainStatus, AppError> {
    walk(conn, 0, GENESIS, i64::MAX, &mut Kinds::new()).await
}

/// Event kind → whether any event of that kind is public.
type Kinds = BTreeMap<String, bool>;

/// Events per statement of [`walk`]; the walk yields between pages.
const WALK_PAGE: usize = 256;

/// Streams events with `after_seq < seq <= until`, continuing from `after_hash`.
/// `Valid.events` is the last verified seq (seqs are contiguous from 1). Kinds seen
/// are collected into `kinds`.
pub(crate) async fn walk(
    conn: &Connection,
    after_seq: i64,
    after_hash: &str,
    until: i64,
    kinds: &mut Kinds,
) -> Result<ChainStatus, AppError> {
    let mut prev = after_hash.to_owned();
    let mut last = after_seq;
    // Short statements: no snapshot held over the whole table (WAL checkpoints
    // proceed) and the worker is released between pages. Rows are append-only,
    // so pages are consistent with each other.
    loop {
        let mut rows = conn
            .query(
                "SELECT seq, occurred_at, kind, visibility, actor_ref, subject, payload, prev_hash, hash
                 FROM audit_events WHERE seq > ?1 AND seq <= ?2 ORDER BY seq LIMIT ?3",
                (last, until, WALK_PAGE as i64),
            )
            .await?;
        let mut in_page = 0;
        while let Some(row) = rows.next().await? {
            in_page += 1;
            let seq: i64 = row.get(0)?;
            let prev_hash: String = row.get(7)?;
            let hash: String = row.get(8)?;
            let (kind, visibility): (String, String) = (row.get(2)?, row.get(3)?);
            let recomputed = chain_hash(
                &prev,
                serde_json::json!([
                    seq,
                    row.get::<String>(1)?,
                    kind,
                    visibility,
                    row.get::<Option<String>>(4)?,
                    row.get::<Option<String>>(5)?,
                    row.get::<String>(6)?,
                ]),
            );
            *kinds.entry(kind).or_default() |= visibility == "public";
            if seq != last + 1 || prev_hash != prev || recomputed != hash {
                return Ok(ChainStatus::Broken { at_seq: seq });
            }
            prev = hash;
            last = seq;
        }
        drop(rows);
        if in_page < WALK_PAGE {
            break;
        }
        tokio::task::yield_now().await;
    }
    Ok(ChainStatus::Valid {
        events: last,
        head: prev,
    })
}

/// Interval of the background full recomputation ([`run_chain_verifier`]);
/// requests only verify new events on top of the last verified head.
pub const FULL_RECHECK: chrono::Duration = chrono::Duration::minutes(10);

/// A request walks the whole chain itself only when the background pass is
/// this late (or has not run yet, e.g. right after startup).
const STALE_FULL_PASS: chrono::Duration = chrono::Duration::minutes(20);

struct Verified {
    seq: i64,
    head: String,
    full_at: DateTime<Utc>,
    kinds: Kinds,
}

/// Chain status plus the event kinds collected while verifying (for filters).
pub struct ChainView {
    pub status: ChainStatus,
    kinds: Kinds,
}

impl ChainView {
    pub fn kinds(&self, scope: Scope) -> Vec<String> {
        self.kinds
            .iter()
            .filter(|(_, public)| scope == Scope::Restricted || **public)
            .map(|(k, _)| k.clone())
            .collect()
    }
}

/// Process-local cache of the last verified head (one process owns the database).
#[derive(Default)]
pub struct ChainCache(tokio::sync::Mutex<Option<Verified>>);

/// The cache shared by the `/auditoria` pages and the background verifier.
pub fn process_chain_cache() -> &'static ChainCache {
    static CACHE: std::sync::LazyLock<ChainCache> = std::sync::LazyLock::new(Default::default);
    &CACHE
}

/// Chain status for display. Incremental on top of the last full pass, so the
/// cost per request is proportional to new events. A rewrite of already verified
/// rows is detected by the next full pass ([`recheck_chain`], every
/// [`FULL_RECHECK`] in the background and at the first request after start).
#[tracing::instrument(level = "debug", skip_all, err(level = "debug"))]
pub async fn summary(
    conn: &Connection,
    cache: &ChainCache,
    now: DateTime<Utc>,
) -> Result<ChainView, AppError> {
    let mut cached = cache.0.lock().await;
    let (status, full_at, mut kinds) = match cached.take() {
        Some(mut v) if now - v.full_at < STALE_FULL_PASS => {
            let status = walk(conn, v.seq, &v.head, i64::MAX, &mut v.kinds).await?;
            (status, v.full_at, v.kinds)
        }
        _ => {
            let mut kinds = Kinds::new();
            (
                walk(conn, 0, GENESIS, i64::MAX, &mut kinds).await?,
                now,
                kinds,
            )
        }
    };
    match &status {
        ChainStatus::Valid { events, head } => {
            *cached = Some(Verified {
                seq: *events,
                head: head.clone(),
                full_at,
                kinds: kinds.clone(),
            });
        }
        ChainStatus::Broken { .. } => {
            kinds = Kinds::new();
            for kind in self::kinds(conn, Scope::Restricted).await? {
                kinds.insert(kind, false);
            }
            for kind in self::kinds(conn, Scope::Public).await? {
                kinds.insert(kind, true);
            }
        }
    }
    Ok(ChainView { status, kinds })
}

/// Full recomputation from genesis, without holding the cache while walking
/// (requests keep being served incrementally). A valid chain replaces the
/// cached head; a broken one clears it, so requests report the break.
#[tracing::instrument(level = "debug", skip_all, err(level = "debug"))]
pub async fn recheck_chain(
    conn: &Connection,
    cache: &ChainCache,
    now: DateTime<Utc>,
) -> Result<ChainStatus, AppError> {
    let mut kinds = Kinds::new();
    let status = walk(conn, 0, GENESIS, i64::MAX, &mut kinds).await?;
    let mut cached = cache.0.lock().await;
    *cached = match &status {
        ChainStatus::Valid { events, head } => Some(Verified {
            seq: *events,
            head: head.clone(),
            full_at: now,
            kinds,
        }),
        ChainStatus::Broken { .. } => None,
    };
    Ok(status)
}

/// Background task: full chain recomputation every [`FULL_RECHECK`].
pub async fn run_chain_verifier(
    db: Db,
    clock: std::sync::Arc<dyn crate::infrastructure::clock::Clock>,
) {
    let mut tick = tokio::time::interval(
        FULL_RECHECK
            .to_std()
            .unwrap_or(std::time::Duration::from_secs(600)),
    );
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tick.tick().await;
        let result = match db.read().await {
            Ok(conn) => recheck_chain(&conn, process_chain_cache(), clock.now()).await,
            Err(e) => Err(e.into()),
        };
        match result {
            Ok(ChainStatus::Valid { .. }) => {}
            Ok(ChainStatus::Broken { at_seq }) => {
                tracing::error!(at_seq, "audit chain broken")
            }
            Err(e) => tracing::warn!(error = %e, "audit chain verification failed"),
        }
    }
}

/// Who is reading: public visitors never receive restricted events or actor refs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Public,
    Restricted,
}

/// Newest-first page of events with `seq < before`, optionally of one kind.
#[tracing::instrument(level = "debug", skip_all, fields(scope = ?scope, kind = ?kind, before = ?before), err(level = "debug"))]
pub async fn list(
    conn: &Connection,
    scope: Scope,
    kind: Option<&str>,
    before: Option<i64>,
) -> Result<(Vec<EventView>, Option<i64>), AppError> {
    if kind.is_some_and(|k| !valid_kind(k)) {
        return Err(AppError::validation("kind", "tipo de evento inválido"));
    }
    let mut sql = String::from(
        "SELECT seq, occurred_at, kind, visibility, actor_ref, subject, payload, hash FROM audit_events WHERE seq < ?1",
    );
    let mut params = vec![Value::Integer(before.unwrap_or(i64::MAX))];
    if scope == Scope::Public {
        sql.push_str(" AND visibility = 'public'");
    }
    if let Some(kind) = kind {
        sql.push_str(" AND kind = ?2");
        params.push(Value::Text(kind.to_owned()));
    }
    sql.push_str(&format!(" ORDER BY seq DESC LIMIT {}", PAGE_SIZE + 1));
    let mut events = Vec::new();
    for row in query_all(conn, &sql, params).await? {
        let restricted = row.get::<String>(3)? == "restricted";
        events.push(EventView {
            seq: row.get(0)?,
            occurred_at: row.get(1)?,
            kind: row.get(2)?,
            restricted,
            actor_ref: match scope {
                Scope::Restricted => row.get(4)?,
                Scope::Public => None,
            },
            subject: row.get(5)?,
            payload: row.get(6)?,
            hash: row.get(7)?,
        });
    }
    let next = (events.len() > PAGE_SIZE as usize).then(|| {
        events.truncate(PAGE_SIZE as usize);
        events.last().map(|e| e.seq)
    });
    Ok((events, next.flatten()))
}

/// Distinct kinds visible in `scope`, for the filter.
#[tracing::instrument(level = "debug", skip_all, fields(scope = ?scope), err(level = "debug"))]
pub async fn kinds(conn: &Connection, scope: Scope) -> Result<Vec<String>, AppError> {
    let sql = match scope {
        Scope::Public => {
            "SELECT DISTINCT kind FROM audit_events WHERE visibility = 'public' ORDER BY kind"
        }
        Scope::Restricted => "SELECT DISTINCT kind FROM audit_events ORDER BY kind",
    };
    query_all(conn, sql, ())
        .await?
        .into_iter()
        .map(|r| r.get::<String>(0).map_err(AppError::from))
        .collect()
}

/// Restricted read for auditors/country admins. The access itself is appended
/// (and committed) before any restricted data is returned; if that fails, the
/// read is denied.
#[tracing::instrument(level = "debug", skip_all, fields(actor = %actor.user_id, kind = ?kind, before = ?before), err(level = "debug"))]
pub async fn read_restricted(
    db: &Db,
    now: DateTime<Utc>,
    actor: &Actor,
    kind: Option<&str>,
    before: Option<i64>,
) -> Result<(Vec<EventView>, Option<i64>), AppError> {
    actor.require(Permission::AuditReadRestricted)?;
    if kind.is_some_and(|k| !valid_kind(k)) {
        return Err(AppError::validation("kind", "tipo de evento inválido"));
    }
    let tx = db.write().await?;
    append(
        &tx,
        now,
        Event {
            kind: RESTRICTED_ACCESS,
            visibility: Visibility::Restricted,
            actor_ref: Some(&actor.user_id),
            subject: Some(&actor.country_id),
            payload: serde_json::json!({ "scope": "country", "kind": kind, "before": before }),
        },
    )
    .await?;
    tx.commit().await?;
    list(&*db.read().await?, Scope::Restricted, kind, before).await
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

    use crate::{
        application::session::{self, SessionPolicy, testing::citizen_with_session},
        infrastructure::catalog,
    };

    fn ev<'a>(kind: &'a str, visibility: Visibility, actor_ref: Option<&'a str>) -> Event<'a> {
        Event {
            kind,
            visibility,
            actor_ref,
            subject: Some("subject"),
            payload: json!({ "n": 1 }),
        }
    }

    async fn actor_with_roles(db: &Db, roles: &[&str]) -> Actor {
        let now = Utc::now();
        let policy = SessionPolicy::from_secs(900, 28800);
        let (user, token) = citizen_with_session(db, now, policy, "4202404").await;
        let tx = db.write().await.unwrap();
        tx.execute(
            "DELETE FROM iam_user_policies WHERE user_id = ?1",
            [user.as_str()],
        )
        .await
        .unwrap();
        for name in roles {
            session::testing::attach_policy(&tx, &user, name).await;
        }
        tx.commit().await.unwrap();
        session::resolve(db, now, policy, &token, false)
            .await
            .unwrap()
            .unwrap()
    }

    #[tokio::test]
    async fn public_listing_hides_restricted_events_and_paginates() {
        let t = migrated().await;
        let now = Utc::now();
        let tx = t.db.write().await.unwrap();
        for i in 0..(PAGE_SIZE + 5) {
            let kind = if i % 2 == 0 { "a.even" } else { "a.odd" };
            append(&tx, now, ev(kind, Visibility::Public, Some("u1")))
                .await
                .unwrap();
            append(&tx, now, ev("b.secret", Visibility::Restricted, Some("u1")))
                .await
                .unwrap();
        }
        tx.commit().await.unwrap();
        let conn = t.db.read().await.unwrap();
        let (page, next) = list(&conn, Scope::Public, None, None).await.unwrap();
        assert_eq!(page.len(), PAGE_SIZE as usize);
        assert!(page.iter().all(|e| !e.restricted && e.actor_ref.is_none()));
        assert!(page.windows(2).all(|w| w[0].seq > w[1].seq));
        let (rest, end) = list(&conn, Scope::Public, None, next).await.unwrap();
        assert_eq!(rest.len(), 5);
        assert_eq!(end, None);
        let (odd, _) = list(&conn, Scope::Public, Some("a.odd"), None)
            .await
            .unwrap();
        assert!(odd.iter().all(|e| e.kind == "a.odd"));
        assert!(
            list(&conn, Scope::Public, Some("b.secret"), None)
                .await
                .unwrap()
                .0
                .is_empty()
        );
        assert!(matches!(
            list(&conn, Scope::Public, Some("x' OR 1=1"), None).await,
            Err(AppError::Validation { .. })
        ));
        assert_eq!(
            kinds(&conn, Scope::Public).await.unwrap(),
            vec!["a.even", "a.odd"]
        );
        let (all, _) = list(&conn, Scope::Restricted, None, None).await.unwrap();
        assert!(
            all.iter()
                .any(|e| e.restricted && e.actor_ref.as_deref() == Some("u1"))
        );
    }

    #[tokio::test]
    async fn restricted_read_requires_permission_and_logs_each_access() {
        let t = migrated().await;
        catalog::import_str(&t.db, "mini", "code,kind,name,parent_code,state_abbrev\nBR,national,Brasil,,\n42,state,SC,BR,SC\n4202404,municipal,Blumenau,42,SC\n").await.unwrap();
        let now = Utc::now();
        let tx = t.db.write().await.unwrap();
        append(&tx, now, ev("b.secret", Visibility::Restricted, Some("u1")))
            .await
            .unwrap();
        tx.commit().await.unwrap();

        let citizen = actor_with_roles(&t.db, &["Cidadao"]).await;
        assert_eq!(
            read_restricted(&t.db, now, &citizen, None, None).await,
            Err(AppError::Forbidden)
        );
        let count = |db: Db| async move {
            match verify(&db.read().await.unwrap()).await.unwrap() {
                ChainStatus::Valid { events, .. } => events,
                other => panic!("{other:?}"),
            }
        };
        assert_eq!(
            count(t.db.clone()).await,
            1,
            "denied read is not logged as access"
        );

        for roles in [&["Auditor"][..], &["AdministratorAccess"][..]] {
            let reader = actor_with_roles(&t.db, roles).await;
            let (events, _) = read_restricted(&t.db, now, &reader, None, None)
                .await
                .unwrap();
            let access = &events[0];
            assert_eq!(
                access.kind, RESTRICTED_ACCESS,
                "access logged before reading"
            );
            assert!(access.restricted);
            assert_eq!(access.actor_ref.as_deref(), Some(reader.user_id.as_str()));
            assert!(events.iter().any(|e| e.kind == "b.secret"));
        }
        assert_eq!(count(t.db.clone()).await, 3);

        // Account status dominates attached policies (applied when the actor loads).
        let auditor = actor_with_roles(&t.db, &["Auditor"]).await;
        let tx = t.db.write().await.unwrap();
        tx.execute(
            "UPDATE users SET status = 'pending' WHERE id = ?1",
            [auditor.user_id.as_str()],
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        let pending_auditor = crate::application::actor::load(
            &t.db.read().await.unwrap(),
            &auditor.user_id,
            &auditor.session_hash,
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(
            read_restricted(&t.db, now, &pending_auditor, None, None).await,
            Err(AppError::Forbidden)
        );
    }

    #[tokio::test]
    async fn forbidden_payload_keys_are_refused() {
        let t = migrated().await;
        let tx = t.db.write().await.unwrap();
        let mut event = ev("x", Visibility::Restricted, None);
        event.payload = json!({ "ok": [{ "user_email": "a@b.c" }] });
        assert_eq!(
            append(&tx, Utc::now(), event).await,
            Err(AppError::Internal)
        );
        tx.rollback().await.unwrap();
    }

    #[tokio::test]
    async fn summary_verifies_incrementally_and_rechecks_fully() {
        let t = migrated().await;
        let cache = ChainCache::default();
        let now = Utc::now();
        let add = |n: usize| {
            let db = t.db.clone();
            async move {
                let tx = db.write().await.unwrap();
                for _ in 0..n {
                    append(&tx, now, ev("k", Visibility::Public, None))
                        .await
                        .unwrap();
                }
                tx.commit().await.unwrap();
            }
        };
        add(3).await;
        let conn = t.db.read().await.unwrap();
        assert!(matches!(
            summary(&conn, &cache, now).await.unwrap().status,
            ChainStatus::Valid { events: 3, .. }
        ));
        add(2).await;
        let tx = t.db.write().await.unwrap();
        append(&tx, now, ev("r.only", Visibility::Restricted, None))
            .await
            .unwrap();
        tx.commit().await.unwrap();
        let incremental = summary(&conn, &cache, now + chrono::Duration::minutes(1))
            .await
            .unwrap();
        assert_eq!(incremental.status, verify(&conn).await.unwrap());
        assert!(matches!(
            incremental.status,
            ChainStatus::Valid { events: 6, .. }
        ));
        assert_eq!(incremental.kinds(Scope::Public), vec!["k"]);
        assert_eq!(incremental.kinds(Scope::Restricted), vec!["k", "r.only"]);
        let full = summary(&conn, &cache, now + FULL_RECHECK).await.unwrap();
        assert_eq!(full.status, incremental.status);
        assert_eq!(
            full.kinds(Scope::Restricted),
            incremental.kinds(Scope::Restricted)
        );
    }

    /// Requests stay incremental while the background pass is recent; the full
    /// pass detects a rewrite of already verified rows and requests then report it.
    #[tokio::test]
    async fn background_full_pass_detects_rewrite_of_verified_rows() {
        let t = migrated().await;
        let cache = ChainCache::default();
        let now = Utc::now();
        let tx = t.db.write().await.unwrap();
        for _ in 0..3 {
            append(&tx, now, ev("k", Visibility::Public, None))
                .await
                .unwrap();
        }
        tx.commit().await.unwrap();
        let conn = t.db.read().await.unwrap();
        assert!(matches!(
            recheck_chain(&conn, &cache, now).await.unwrap(),
            ChainStatus::Valid { events: 3, .. }
        ));
        // Simulated tampering of a verified row (the trigger blocks normal updates).
        conn.execute("DROP TRIGGER audit_events_no_update", ())
            .await
            .unwrap();
        conn.execute("UPDATE audit_events SET payload = '{}' WHERE seq = 2", ())
            .await
            .unwrap();
        let later = now + FULL_RECHECK + chrono::Duration::minutes(1);
        assert!(
            matches!(
                summary(&conn, &cache, later).await.unwrap().status,
                ChainStatus::Valid { events: 3, .. }
            ),
            "request path only walks new events"
        );
        assert_eq!(
            recheck_chain(&conn, &cache, later).await.unwrap(),
            ChainStatus::Broken { at_seq: 2 }
        );
        assert_eq!(
            summary(&conn, &cache, later).await.unwrap().status,
            ChainStatus::Broken { at_seq: 2 }
        );
    }

    async fn append_n(db: &Db, n: usize) {
        let now = Utc::now();
        let tx = db.write().await.unwrap();
        for _ in 0..n {
            append(&tx, now, ev("k", Visibility::Public, None))
                .await
                .unwrap();
        }
        tx.commit().await.unwrap();
    }

    /// Regression: a full walk over many events lets other tasks on the same
    /// worker run between pages instead of holding the thread to the end.
    #[tokio::test(flavor = "current_thread")]
    async fn full_walk_yields_to_other_tasks() {
        let t = migrated().await;
        append_n(&t.db, WALK_PAGE * 4 + 3).await;
        let conn = t.db.read().await.unwrap();
        let _ = verify(&conn).await.unwrap(); // warm the page cache
        let runs = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counter = runs.clone();
        let other = tokio::spawn(async move {
            loop {
                counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                tokio::task::yield_now().await;
            }
        });
        tokio::task::yield_now().await;
        let before = runs.load(std::sync::atomic::Ordering::SeqCst);
        let status = verify(&conn).await.unwrap();
        let during = runs.load(std::sync::atomic::Ordering::SeqCst) - before;
        other.abort();
        assert!(
            matches!(status, ChainStatus::Valid { events, .. } if events == (WALK_PAGE * 4 + 3) as i64)
        );
        assert!(during >= 4, "other task ran {during} times during the walk");
    }

    /// Tampering right after a page boundary is still detected at its seq.
    #[tokio::test]
    async fn walk_detects_tampering_across_pages() {
        let t = migrated().await;
        append_n(&t.db, WALK_PAGE * 2 + 1).await;
        let conn = t.db.read().await.unwrap();
        conn.execute("DROP TRIGGER audit_events_no_update", ())
            .await
            .unwrap();
        let seq = WALK_PAGE as i64 + 1;
        conn.execute(
            "UPDATE audit_events SET payload = '{}' WHERE seq = ?1",
            [seq],
        )
        .await
        .unwrap();
        assert_eq!(
            verify(&conn).await.unwrap(),
            ChainStatus::Broken { at_seq: seq }
        );
        let mut kinds = Kinds::new();
        assert!(
            matches!(
                walk(&conn, 0, GENESIS, WALK_PAGE as i64, &mut kinds).await.unwrap(),
                ChainStatus::Valid { events, .. } if events == WALK_PAGE as i64
            ),
            "`until` stops before the tampered row"
        );
    }

    /// Timing evidence: `cargo test --release --features ssr verify_100k -- --ignored --nocapture`.
    #[tokio::test]
    #[ignore = "performance measurement"]
    async fn verify_100k_events_timing() {
        let t = migrated().await;
        let now = Utc::now();
        let start = std::time::Instant::now();
        let tx = t.db.write().await.unwrap();
        for i in 0..100_000 {
            let mut e = ev("proposal.created", Visibility::Public, None);
            e.payload = json!({ "proposal_id": i, "text_hash": "ab".repeat(32) });
            append(&tx, now, e).await.unwrap();
        }
        tx.commit().await.unwrap();
        let appended = start.elapsed();
        let conn = t.db.read().await.unwrap();
        let start = std::time::Instant::now();
        let status = verify(&conn).await.unwrap();
        let full = start.elapsed();
        assert!(matches!(
            status,
            ChainStatus::Valid {
                events: 100_000,
                ..
            }
        ));
        let cache = ChainCache::default();
        let start = std::time::Instant::now();
        summary(&conn, &cache, now).await.unwrap();
        let first_summary = start.elapsed();
        let tx = t.db.write().await.unwrap();
        append(&tx, now, ev("k", Visibility::Public, None))
            .await
            .unwrap();
        tx.commit().await.unwrap();
        let start = std::time::Instant::now();
        let status = summary(&conn, &cache, now).await.unwrap().status;
        let incremental = start.elapsed();
        assert!(matches!(
            status,
            ChainStatus::Valid {
                events: 100_001,
                ..
            }
        ));
        let start = std::time::Instant::now();
        let (page, _) = list(&conn, Scope::Public, Some("k"), None).await.unwrap();
        let filtered = start.elapsed();
        assert_eq!(page.len(), 1);
        let start = std::time::Instant::now();
        let (page, _) = list(&conn, Scope::Public, None, None).await.unwrap();
        let unfiltered = start.elapsed();
        assert_eq!(page.len(), PAGE_SIZE as usize);
        let start = std::time::Instant::now();
        assert_eq!(kinds(&conn, Scope::Public).await.unwrap().len(), 2);
        let kinds_time = start.elapsed();
        println!(
            "100k audit events: append {appended:?}, full verify {full:?}, first cached summary {first_summary:?}, incremental verify (+1) {incremental:?}, \
             first page {unfiltered:?}, rare-kind page {filtered:?}, distinct kinds {kinds_time:?}"
        );
    }
}
