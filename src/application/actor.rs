use crate::{
    domain::{
        iam::{ANY_RESOURCE, Access, Permission},
        identity::AccountStatus,
    },
    error::AppError,
    infrastructure::db::query_opt,
};

/// Authenticated caller resolved on the server from the session cookie. `access`
/// holds the IAM policies in force, loaded on every request (never cached).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Actor {
    pub user_id: String,
    pub session_hash: String,
    pub name: String,
    pub status: AccountStatus,
    pub access: Access,
    pub municipality_id: String,
    pub state_id: String,
    pub country_id: String,
}

impl Actor {
    /// Unscoped check (request resource `*`).
    pub fn can(&self, permission: Permission) -> bool {
        self.access.allows(permission, ANY_RESOURCE)
    }

    pub fn require(&self, permission: Permission) -> Result<(), AppError> {
        if self.can(permission) {
            Ok(())
        } else {
            Err(AppError::Forbidden)
        }
    }

    /// Resource-scoped check, e.g. `jurisdiction:<IBGE code>`.
    pub fn can_on(&self, permission: Permission, resource: &str) -> bool {
        self.access.allows(permission, resource)
    }

    /// Jurisdictions where this account participates: municipality, state, country.
    pub fn participates_in(&self, jurisdiction_id: &str) -> bool {
        [&self.municipality_id, &self.state_id, &self.country_id]
            .iter()
            .any(|j| j.as_str() == jurisdiction_id)
    }
}

/// Account row plus its effective policies (two queries).
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
    let status = AccountStatus::from_db(&row.get::<String>(1)?)
        .ok_or_else(|| AppError::internal("missing or invalid stored value"))?;
    let access = super::iam::principal_access(conn, user_id, status).await?;
    Ok(Some(Actor {
        user_id: user_id.to_owned(),
        session_hash: session_hash.to_owned(),
        name: row.get(0)?,
        status,
        access,
        municipality_id: row.get(2)?,
        state_id: row.get(3)?,
        country_id: row.get(4)?,
    }))
}
