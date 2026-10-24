//! In-app notifications written inside the caller's transaction, honouring the
//! recipient's per-category preferences. Email is not delivered by this version
//! (no sender configured); there is no outbox.

use chrono::{DateTime, Utc};
use turso::Connection;

use crate::{
    domain::notification::{Category, NotificationPage, NotificationView, PAGE_SIZE, Preference},
    error::AppError,
    infrastructure::{
        clock::ts,
        crypto::new_id,
        db::{Db, query_all, query_opt},
    },
};

pub struct Notice<'a> {
    /// A [`Category::as_db`] value; other events (e.g. security notices) are
    /// always delivered.
    pub event: &'a str,
    pub title: &'a str,
    pub body: &'a str,
    pub link: Option<&'a str>,
}

/// Stores the notice unless the user disabled its category. Call inside the
/// write transaction of the effect that caused it.
pub async fn notify(
    conn: &Connection,
    now: DateTime<Utc>,
    user_id: &str,
    notice: Notice<'_>,
) -> Result<(), AppError> {
    deliver(conn, now, user_id, notice).await.map(|_| ())
}

/// Returns whether the notice was stored (false when the category is disabled).
async fn deliver(
    conn: &Connection,
    now: DateTime<Utc>,
    user_id: &str,
    notice: Notice<'_>,
) -> Result<bool, AppError> {
    if Category::from_db(notice.event).is_some()
        && query_opt(
            conn,
            "SELECT 1 FROM notification_preferences WHERE user_id = ?1 AND category = ?2 AND in_app = 0",
            (user_id, notice.event),
        )
        .await?
        .is_some()
    {
        return Ok(false);
    }
    conn.execute(
        "INSERT INTO notifications (id, user_id, event, title, body, link, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        (new_id(), user_id, notice.event, notice.title, notice.body, notice.link, ts(now)),
    )
    .await?;
    Ok(true)
}

pub async fn unread_count(conn: &Connection, user_id: &str) -> Result<i64, AppError> {
    Ok(query_opt(
        conn,
        "SELECT COUNT(*) FROM notifications WHERE user_id = ?1 AND read_at IS NULL",
        [user_id],
    )
    .await?
    .map(|r| r.get::<i64>(0))
    .transpose()?
    .unwrap_or(0))
}

/// Newest-first page (0-based) of the user's own notifications.
pub async fn list(
    conn: &Connection,
    user_id: &str,
    page: u32,
) -> Result<NotificationPage, AppError> {
    let rows = query_all(
        conn,
        &format!(
            "SELECT id, event, title, body, link, created_at, read_at FROM notifications
             WHERE user_id = ?1 ORDER BY created_at DESC, id DESC LIMIT {} OFFSET {}",
            PAGE_SIZE + 1,
            u64::from(page) * u64::from(PAGE_SIZE)
        ),
        [user_id],
    )
    .await?;
    let mut items = rows
        .into_iter()
        .map(|r| {
            Ok(NotificationView {
                id: r.get(0)?,
                event: r.get(1)?,
                title: r.get(2)?,
                body: r.get(3)?,
                link: r.get(4)?,
                created_at: r.get(5)?,
                read: r.get::<Option<String>>(6)?.is_some(),
            })
        })
        .collect::<Result<Vec<_>, AppError>>()?;
    let has_more = items.len() > PAGE_SIZE as usize;
    items.truncate(PAGE_SIZE as usize);
    Ok(NotificationPage {
        items,
        unread: unread_count(conn, user_id).await?,
        has_more,
    })
}

/// Marks one of the user's notifications as read. Another user's (or an unknown)
/// id is `NotFound`, without revealing whether it exists.
pub async fn mark_read(
    conn: &Connection,
    now: DateTime<Utc>,
    user_id: &str,
    id: &str,
) -> Result<(), AppError> {
    conn.execute(
        "UPDATE notifications SET read_at = ?3 WHERE id = ?1 AND user_id = ?2 AND read_at IS NULL",
        (id, user_id, ts(now)),
    )
    .await?;
    query_opt(
        conn,
        "SELECT 1 FROM notifications WHERE id = ?1 AND user_id = ?2",
        (id, user_id),
    )
    .await?
    .map(|_| ())
    .ok_or(AppError::NotFound)
}

pub async fn mark_all_read(
    conn: &Connection,
    now: DateTime<Utc>,
    user_id: &str,
) -> Result<u64, AppError> {
    Ok(conn
        .execute(
            "UPDATE notifications SET read_at = ?2 WHERE user_id = ?1 AND read_at IS NULL",
            (user_id, ts(now)),
        )
        .await?)
}

/// Every category with its in-app state (absent row = enabled).
pub async fn preferences(conn: &Connection, user_id: &str) -> Result<Vec<Preference>, AppError> {
    let disabled: Vec<String> = query_all(
        conn,
        "SELECT category FROM notification_preferences WHERE user_id = ?1 AND in_app = 0",
        [user_id],
    )
    .await?
    .into_iter()
    .map(|r| r.get::<String>(0))
    .collect::<Result<_, _>>()?;
    Ok(Category::ALL
        .into_iter()
        .map(|category| Preference {
            category,
            in_app: !disabled.iter().any(|d| d == category.as_db()),
        })
        .collect())
}

pub async fn set_preference(
    conn: &Connection,
    now: DateTime<Utc>,
    user_id: &str,
    category: Category,
    in_app: bool,
) -> Result<(), AppError> {
    conn.execute(
        "INSERT INTO notification_preferences (user_id, category, in_app, updated_at) VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(user_id, category) DO UPDATE SET in_app = excluded.in_app, updated_at = excluded.updated_at",
        (user_id, category.as_db(), in_app as i64, ts(now)),
    )
    .await?;
    Ok(())
}

/// Recipients of a scheduled notice.
#[derive(Debug, Clone, Copy)]
pub enum Audience<'a> {
    User(&'a str),
    /// Every active account participating in the jurisdiction (municipality,
    /// state or country), resolved at delivery time.
    Jurisdiction(&'a str),
}

/// Schedules a notice for `due_at` (e.g. a deadline reminder). Idempotent by
/// `dedupe_key`: returns false when that key was already scheduled. Call inside
/// the transaction that sets the deadline.
pub async fn schedule(
    conn: &Connection,
    now: DateTime<Utc>,
    dedupe_key: &str,
    audience: Audience<'_>,
    notice: Notice<'_>,
    due_at: DateTime<Utc>,
) -> Result<bool, AppError> {
    let (user, jurisdiction) = match audience {
        Audience::User(id) => (Some(id), None),
        Audience::Jurisdiction(id) => (None, Some(id)),
    };
    Ok(conn
        .execute(
            "INSERT INTO scheduled_notifications (id, dedupe_key, user_id, jurisdiction_id, event, title, body, link, due_at, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10) ON CONFLICT(dedupe_key) DO NOTHING",
            (
                new_id(),
                dedupe_key,
                user,
                jurisdiction,
                notice.event,
                notice.title,
                notice.body,
                notice.link,
                ts(due_at),
                ts(now),
            ),
        )
        .await?
        == 1)
}

/// Drops a not yet delivered scheduled notice (e.g. deadline moved/cancelled).
pub async fn cancel_scheduled(conn: &Connection, dedupe_key: &str) -> Result<bool, AppError> {
    Ok(conn
        .execute(
            "DELETE FROM scheduled_notifications WHERE dedupe_key = ?1 AND delivered_at IS NULL",
            [dedupe_key],
        )
        .await?
        == 1)
}

/// Delivers every scheduled notice due at `now`, once each (one transaction per
/// scheduled row). Returns the number of notifications stored. Run periodically.
pub async fn remind_due(db: &Db, now: DateTime<Utc>) -> Result<u64, AppError> {
    let due = query_all(
        &db.read().await?,
        "SELECT id FROM scheduled_notifications WHERE delivered_at IS NULL AND due_at <= ?1 ORDER BY due_at LIMIT 100",
        [ts(now)],
    )
    .await?;
    let mut stored = 0;
    for row in due {
        let id: String = row.get(0)?;
        let tx = db.write().await?;
        let Some(item) = query_opt(
            &tx,
            "SELECT user_id, jurisdiction_id, event, title, body, link FROM scheduled_notifications WHERE id = ?1 AND delivered_at IS NULL",
            [id.as_str()],
        )
        .await?
        else {
            continue;
        };
        let (user, jurisdiction): (Option<String>, Option<String>) = (item.get(0)?, item.get(1)?);
        let recipients: Vec<String> = match (user, jurisdiction) {
            (Some(user), _) => vec![user],
            (None, Some(j)) => query_all(
                &tx,
                "SELECT id FROM users WHERE status = 'active' AND (municipality_id = ?1 OR state_id = ?1 OR country_id = ?1)",
                [j.as_str()],
            )
            .await?
            .into_iter()
            .map(|r| r.get::<String>(0))
            .collect::<Result<_, _>>()?,
            (None, None) => Vec::new(),
        };
        let (event, title, body, link): (String, String, String, Option<String>) =
            (item.get(2)?, item.get(3)?, item.get(4)?, item.get(5)?);
        for user in &recipients {
            let stored_one = deliver(
                &tx,
                now,
                user,
                Notice {
                    event: &event,
                    title: &title,
                    body: &body,
                    link: link.as_deref(),
                },
            )
            .await?;
            stored += stored_one as u64;
        }
        tx.execute(
            "UPDATE scheduled_notifications SET delivered_at = ?2 WHERE id = ?1",
            (id.as_str(), ts(now)),
        )
        .await?;
        tx.commit().await?;
    }
    Ok(stored)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        application::session::{SessionPolicy, testing::citizen_with_session},
        infrastructure::{catalog, db::testing::migrated},
    };

    const MINI: &str = "code,kind,name,parent_code,state_abbrev\nBR,national,Brasil,,\n42,state,SC,BR,SC\n4202404,municipal,Blumenau,42,SC\n4205407,municipal,Florianópolis,42,SC\n";

    fn notice(event: &str) -> Notice<'_> {
        Notice {
            event,
            title: "t",
            body: "b",
            link: None,
        }
    }

    async fn setup() -> (
        crate::infrastructure::db::testing::TestDb,
        DateTime<Utc>,
        String,
        String,
    ) {
        let t = migrated().await;
        catalog::import_str(&t.db, "mini", MINI).await.unwrap();
        let now: DateTime<Utc> = "2026-10-24T12:00:00Z".parse().unwrap();
        let policy = SessionPolicy::from_secs(900, 28800);
        let (a, _) = citizen_with_session(&t.db, now, policy, "4202404").await;
        let (b, _) = citizen_with_session(&t.db, now, policy, "4205407").await;
        (t, now, a, b)
    }

    #[tokio::test]
    async fn preferences_filter_categories_but_not_uncategorized_events() {
        let (t, now, a, _) = setup().await;
        let tx = t.db.write().await.unwrap();
        assert!(preferences(&tx, &a).await.unwrap().iter().all(|p| p.in_app));
        set_preference(&tx, now, &a, Category::ProposalCreated, false)
            .await
            .unwrap();
        notify(&tx, now, &a, notice("proposal_created"))
            .await
            .unwrap();
        notify(&tx, now, &a, notice("voting_opened")).await.unwrap();
        notify(&tx, now, &a, notice("security_notice"))
            .await
            .unwrap();
        let events: Vec<String> = list(&tx, &a, 0)
            .await
            .unwrap()
            .items
            .into_iter()
            .map(|n| n.event)
            .collect();
        assert_eq!(events.len(), 2);
        assert!(!events.contains(&"proposal_created".to_owned()));
        set_preference(&tx, now, &a, Category::ProposalCreated, true)
            .await
            .unwrap();
        notify(&tx, now, &a, notice("proposal_created"))
            .await
            .unwrap();
        assert_eq!(unread_count(&tx, &a).await.unwrap(), 3);
        let prefs = preferences(&tx, &a).await.unwrap();
        assert_eq!(prefs.len(), Category::ALL.len());
        tx.commit().await.unwrap();
    }

    #[tokio::test]
    async fn mark_read_is_limited_to_own_notifications() {
        let (t, now, a, b) = setup().await;
        let tx = t.db.write().await.unwrap();
        notify(&tx, now, &a, notice("voting_opened")).await.unwrap();
        notify(&tx, now, &a, notice("phase_changed")).await.unwrap();
        notify(&tx, now, &b, notice("phase_changed")).await.unwrap();
        let mine = list(&tx, &a, 0).await.unwrap();
        let id = mine.items[0].id.clone();
        assert_eq!(
            mark_read(&tx, now, &b, &id).await,
            Err(AppError::NotFound),
            "other user's notification"
        );
        assert_eq!(unread_count(&tx, &a).await.unwrap(), 2);
        assert_eq!(
            mark_read(&tx, now, &a, "unknown").await,
            Err(AppError::NotFound)
        );
        mark_read(&tx, now, &a, &id).await.unwrap();
        mark_read(&tx, now, &a, &id).await.unwrap(); // idempotent
        assert_eq!(unread_count(&tx, &a).await.unwrap(), 1);
        assert_eq!(mark_all_read(&tx, now, &a).await.unwrap(), 1);
        assert_eq!(unread_count(&tx, &a).await.unwrap(), 0);
        assert_eq!(unread_count(&tx, &b).await.unwrap(), 1, "b untouched");
        tx.commit().await.unwrap();
    }

    #[tokio::test]
    async fn list_paginates_newest_first() {
        let (t, now, a, _) = setup().await;
        let tx = t.db.write().await.unwrap();
        for i in 0..(PAGE_SIZE + 3) {
            notify(
                &tx,
                now + chrono::Duration::seconds(i.into()),
                &a,
                notice("phase_changed"),
            )
            .await
            .unwrap();
        }
        let first = list(&tx, &a, 0).await.unwrap();
        assert!(first.has_more);
        assert_eq!(first.items.len(), PAGE_SIZE as usize);
        assert!(first.items[0].created_at > first.items[1].created_at);
        let second = list(&tx, &a, 1).await.unwrap();
        assert!(!second.has_more);
        assert_eq!(second.items.len(), 3);
        tx.commit().await.unwrap();
    }

    #[tokio::test]
    async fn remind_due_delivers_once_to_audience_respecting_preferences() {
        let (t, now, a, b) = setup().await;
        let state_id: String = query_opt(
            &t.db.read().await.unwrap(),
            "SELECT id FROM jurisdictions WHERE code = '42'",
            (),
        )
        .await
        .unwrap()
        .unwrap()
        .get(0)
        .unwrap();
        let due = now + chrono::Duration::hours(1);
        let tx = t.db.write().await.unwrap();
        set_preference(&tx, now, &b, Category::DeadlineReminder, false)
            .await
            .unwrap();
        let reminder = || Notice {
            event: Category::DeadlineReminder.as_db(),
            title: "Prazo",
            body: "Faltam 24 h",
            link: Some("/propostas/x"),
        };
        assert!(
            schedule(
                &tx,
                now,
                "p1:24h",
                Audience::Jurisdiction(&state_id),
                reminder(),
                due
            )
            .await
            .unwrap()
        );
        assert!(
            !schedule(
                &tx,
                now,
                "p1:24h",
                Audience::Jurisdiction(&state_id),
                reminder(),
                due
            )
            .await
            .unwrap(),
            "deduplicated"
        );
        assert!(
            schedule(&tx, now, "p2:a", Audience::User(&a), reminder(), due)
                .await
                .unwrap()
        );
        assert!(
            schedule(&tx, now, "p3:cancel", Audience::User(&a), reminder(), due)
                .await
                .unwrap()
        );
        assert!(cancel_scheduled(&tx, "p3:cancel").await.unwrap());
        tx.commit().await.unwrap();

        assert_eq!(remind_due(&t.db, now).await.unwrap(), 0, "not due yet");
        assert_eq!(
            remind_due(&t.db, due).await.unwrap(),
            2,
            "a twice, b opted out"
        );
        assert_eq!(remind_due(&t.db, due).await.unwrap(), 0, "delivered once");
        let conn = t.db.read().await.unwrap();
        assert_eq!(unread_count(&conn, &a).await.unwrap(), 2);
        assert_eq!(unread_count(&conn, &b).await.unwrap(), 0);
    }
}
