//! In-app notifications written inside the caller's transaction.

use turso::Connection;

use crate::{
    error::AppError,
    infrastructure::{clock::ts, crypto::new_id},
};

pub struct Notice<'a> {
    pub event: &'a str,
    pub title: &'a str,
    pub body: &'a str,
    pub link: Option<&'a str>,
}

pub async fn notify(
    conn: &Connection,
    now: chrono::DateTime<chrono::Utc>,
    user_id: &str,
    notice: Notice<'_>,
) -> Result<(), AppError> {
    conn.execute(
        "INSERT INTO notifications (id, user_id, event, title, body, link, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        (new_id(), user_id, notice.event, notice.title, notice.body, notice.link, ts(now)),
    )
    .await?;
    Ok(())
}
