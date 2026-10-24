//! Profile: name, private read-only address, privacy preferences. The address and
//! territory cannot change in this version: any attempt is rejected here and by
//! database triggers.

use turso::Connection;

use crate::{
    application::{
        actor::Actor,
        audit::{self, Event},
        reauth,
    },
    domain::identity::{Permission, Visibility, normalize_name},
    error::AppError,
    i18n::t,
    infrastructure::{clock::ts, db::query_opt},
};

pub const PRIVACY_ACTION: &str = "profile:privacy";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Address {
    pub cep: String,
    pub state: String,
    pub city: String,
    pub neighborhood: String,
    pub street: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profile {
    pub name: String,
    pub email: String,
    pub address: Address,
    pub municipality: String,
    pub state: String,
    pub country: String,
    pub profile_visibility: Visibility,
    pub default_vote_visibility: Visibility,
}

pub async fn load(conn: &Connection, user_id: &str) -> Result<Profile, AppError> {
    let row = query_opt(
        conn,
        "SELECT u.name, u.email, a.cep, a.state, a.city, a.neighborhood, a.street, m.name, s.name, n.name,
                COALESCE(p.profile_visibility, 'private'), COALESCE(p.default_vote_visibility, 'private')
         FROM users u
         JOIN addresses a ON a.user_id = u.id
         JOIN jurisdictions m ON m.id = u.municipality_id
         JOIN jurisdictions s ON s.id = u.state_id
         JOIN jurisdictions n ON n.id = u.country_id
         LEFT JOIN user_preferences p ON p.user_id = u.id
         WHERE u.id = ?1",
        [user_id],
    )
    .await?
    .ok_or(AppError::NotFound)?;
    let vis = |i| -> Result<Visibility, AppError> {
        Visibility::from_db(&row.get::<String>(i)?).ok_or(AppError::Internal)
    };
    Ok(Profile {
        name: row.get(0)?,
        email: row.get(1)?,
        address: Address {
            cep: row.get(2)?,
            state: row.get(3)?,
            city: row.get(4)?,
            neighborhood: row.get(5)?,
            street: row.get(6)?,
        },
        municipality: row.get(7)?,
        state: row.get(8)?,
        country: row.get(9)?,
        profile_visibility: vis(10)?,
        default_vote_visibility: vis(11)?,
    })
}

/// Changes requested for the own profile. Address fields exist only to be rejected
/// explicitly: the initial address is immutable in this version.
#[derive(Debug, Default, Clone)]
pub struct ProfileUpdate {
    pub name: Option<String>,
    pub cep: Option<String>,
}

pub async fn update(
    conn: &Connection,
    now: chrono::DateTime<chrono::Utc>,
    actor: &Actor,
    update: ProfileUpdate,
) -> Result<(), AppError> {
    actor.require(Permission::ProfileUpdate)?;
    if update.cep.is_some() {
        return Err(AppError::Conflict(t("profile.address_locked").into()));
    }
    if let Some(name) = update.name {
        let name = normalize_name(&name)
            .ok_or_else(|| AppError::validation("name", t("auth.name.invalid")))?;
        conn.execute(
            "UPDATE users SET name = ?2, updated_at = ?3 WHERE id = ?1",
            (actor.user_id.as_str(), name, ts(now)),
        )
        .await?;
    }
    Ok(())
}

/// Critical action: consumes a `profile:privacy` reauthentication grant in the same
/// transaction as the change.
pub async fn set_privacy(
    conn: &Connection,
    now: chrono::DateTime<chrono::Utc>,
    actor: &Actor,
    profile_visibility: Visibility,
    default_vote_visibility: Visibility,
) -> Result<(), AppError> {
    actor.require(Permission::ProfileUpdate)?;
    reauth::consume(
        conn,
        now,
        &actor.session_hash,
        PRIVACY_ACTION,
        &actor.user_id,
    )
    .await?;
    conn.execute(
        "INSERT INTO user_preferences (user_id, profile_visibility, default_vote_visibility, updated_at) VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(user_id) DO UPDATE SET profile_visibility = excluded.profile_visibility,
           default_vote_visibility = excluded.default_vote_visibility, updated_at = excluded.updated_at",
        (actor.user_id.as_str(), profile_visibility.as_db(), default_vote_visibility.as_db(), ts(now)),
    )
    .await?;
    audit::append(
        conn,
        now,
        Event {
            kind: "identity.privacy_changed",
            visibility: audit::Visibility::Restricted,
            actor_ref: Some(&actor.user_id),
            subject: Some(&actor.user_id),
            payload: serde_json::json!({}),
        },
    )
    .await?;
    Ok(())
}

/// Default vote visibility for the voting feature (private when unset).
pub async fn default_vote_visibility(
    conn: &Connection,
    user_id: &str,
) -> Result<Visibility, AppError> {
    Ok(query_opt(
        conn,
        "SELECT default_vote_visibility FROM user_preferences WHERE user_id = ?1",
        [user_id],
    )
    .await?
    .map(|r| r.get::<String>(0))
    .transpose()?
    .and_then(|v| Visibility::from_db(&v))
    .unwrap_or_default())
}

/// Public view of an account: name and municipality only when the profile is
/// public; `None` for private, pending or blocked accounts. Never the address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicProfile {
    pub name: String,
    pub municipality: String,
}

pub async fn public_view(
    conn: &Connection,
    user_id: &str,
) -> Result<Option<PublicProfile>, AppError> {
    let row = query_opt(
        conn,
        "SELECT u.name, m.name FROM users u JOIN jurisdictions m ON m.id = u.municipality_id
         JOIN user_preferences p ON p.user_id = u.id
         WHERE u.id = ?1 AND u.status = 'active' AND p.profile_visibility = 'public'",
        [user_id],
    )
    .await?;
    row.map(|r| {
        Ok(PublicProfile {
            name: r.get(0)?,
            municipality: r.get(1)?,
        })
    })
    .transpose()
}

#[cfg(test)]
mod tests {
    use super::{
        super::{
            enroll::tests::{actor_for, enrolled},
            testing::*,
        },
        *,
    };

    #[tokio::test]
    async fn address_cannot_change_and_privacy_needs_reauth() {
        let w = World::new().await;
        let e = enrolled(&w, "ana@example.org").await;
        let actor = actor_for(&w, &e.session_token).await;
        let tx = w.t.db.write().await.unwrap();
        let attempt = update(
            &tx,
            w.now,
            &actor,
            ProfileUpdate {
                name: Some("Outro Nome".into()),
                cep: Some("88010000".into()),
            },
        )
        .await;
        assert!(matches!(attempt, Err(AppError::Conflict(_))));
        update(
            &tx,
            w.now,
            &actor,
            ProfileUpdate {
                name: Some("Ana Renomeada".into()),
                cep: None,
            },
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        let conn = w.t.db.read().await.unwrap();
        let p = load(&conn, &e.user_id).await.unwrap();
        assert_eq!(
            (
                p.name.as_str(),
                p.address.cep.as_str(),
                p.municipality.as_str()
            ),
            ("Ana Renomeada", "89010025", "Blumenau")
        );
        assert_eq!(p.default_vote_visibility, Visibility::Private);
        assert_eq!(
            public_view(&conn, &e.user_id).await.unwrap(),
            None,
            "private by default"
        );

        let tx = w.t.db.write().await.unwrap();
        assert_eq!(
            set_privacy(&tx, w.now, &actor, Visibility::Public, Visibility::Public).await,
            Err(AppError::ReauthRequired)
        );
        reauth::grant(
            &tx,
            w.now,
            &actor.session_hash,
            PRIVACY_ACTION,
            &actor.user_id,
        )
        .await
        .unwrap();
        set_privacy(&tx, w.now, &actor, Visibility::Public, Visibility::Public)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        assert_eq!(
            default_vote_visibility(&conn, &e.user_id).await.unwrap(),
            Visibility::Public
        );
        assert_eq!(
            public_view(&conn, &e.user_id)
                .await
                .unwrap()
                .unwrap()
                .municipality,
            "Blumenau"
        );
    }
}
