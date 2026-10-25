use crate::{
    domain::identity::{AccountStatus, Permission, Role, permissions},
    error::AppError,
    infrastructure::db::{query_all, query_opt},
};

/// Authenticated caller resolved on the server from the session cookie.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Actor {
    pub user_id: String,
    pub session_hash: String,
    pub name: String,
    pub status: AccountStatus,
    pub roles: Vec<Role>,
    pub municipality_id: String,
    pub state_id: String,
    pub country_id: String,
}

impl Actor {
    pub fn can(&self, permission: Permission) -> bool {
        permissions(self.status, &self.roles).contains(&permission)
    }

    pub fn require(&self, permission: Permission) -> Result<(), AppError> {
        if self.can(permission) {
            Ok(())
        } else {
            Err(AppError::Forbidden)
        }
    }

    pub fn has_role(&self, role: Role) -> bool {
        self.roles.contains(&role)
    }

    /// Jurisdictions where this account participates: municipality, state, country.
    pub fn participates_in(&self, jurisdiction_id: &str) -> bool {
        [&self.municipality_id, &self.state_id, &self.country_id]
            .iter()
            .any(|j| j.as_str() == jurisdiction_id)
    }
}

#[tracing::instrument(level = "debug", skip_all, fields(user_id = %user_id), err(level = "debug"))]
pub async fn load(
    conn: &turso::Connection,
    user_id: &str,
    session_hash: &str,
) -> Result<Option<Actor>, AppError> {
    let Some(row) = query_opt(
        conn,
        "SELECT name, status, municipality_id, state_id, country_id FROM users WHERE id = ?1",
        [user_id],
    )
    .await?
    else {
        return Ok(None);
    };
    let status: String = row.get(1)?;
    let roles = query_all(
        conn,
        "SELECT role FROM user_roles WHERE user_id = ?1",
        [user_id],
    )
    .await?
    .into_iter()
    .filter_map(|r| r.get::<String>(0).ok().and_then(|v| Role::from_db(&v)))
    .collect();
    Ok(Some(Actor {
        user_id: user_id.to_owned(),
        session_hash: session_hash.to_owned(),
        name: row.get(0)?,
        status: AccountStatus::from_db(&status)
            .ok_or_else(|| AppError::internal("missing or invalid stored value"))?,
        roles,
        municipality_id: row.get(2)?,
        state_id: row.get(3)?,
        country_id: row.get(4)?,
    }))
}
