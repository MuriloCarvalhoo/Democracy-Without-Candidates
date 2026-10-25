//! IAM use cases: effective access recomputed per request, policy/group/attachment
//! management and the policy simulator. Every change requires `iam:manage`, consumes
//! an `iam:manage` reauthentication grant in the same transaction, is audited and
//! may not leave the instance without an AdministratorAccess holder able to manage IAM.

use chrono::{DateTime, Utc};
use serde_json::json;
use turso::Connection;

use super::{
    actor::Actor,
    audit::{self, Event, Visibility},
    reauth,
};
use crate::{
    domain::{
        iam::{
            ADMINISTRATOR_ACCESS, ANY_RESOURCE, Access, CIDADAO, DESCRIPTION_MAX, Effect, GroupRef,
            GroupSummary, GroupView, PUBLICO, Permission, PolicyDocument, PolicyRef, PolicySummary,
            PolicyView, REAUTH_IAM, REAUTH_IAM_RESOURCE, Simulation, Statement, UserRef, UserView,
            jurisdiction_resource, parse_document, valid_name,
        },
        identity::{AccountStatus, normalize_email},
    },
    error::AppError,
    i18n::t,
    infrastructure::{
        clock::ts,
        crypto::new_id,
        db::{Db, WriteTx, is_constraint, query_all, query_opt},
    },
};

/// In-code baseline of every pending/active account: read its own profile.
pub const ACCOUNT_BASELINE: &str = "Conta";

fn baseline() -> (String, PolicyDocument) {
    (
        ACCOUNT_BASELINE.to_owned(),
        PolicyDocument {
            statements: vec![Statement {
                sid: Some("PerfilProprio".into()),
                effect: Effect::Allow,
                actions: vec![Permission::ProfileRead.as_str().into()],
                resources: vec![ANY_RESOURCE.into()],
            }],
        },
    )
}

/// `Publico` plus policies attached to the user directly or through groups.
const ATTACHED_SQL: &str = "SELECT p.name, p.document FROM iam_policies p
    WHERE p.id = ?2
       OR EXISTS (SELECT 1 FROM iam_user_policies up WHERE up.user_id = ?1 AND up.policy_id = p.id)
       OR EXISTS (SELECT 1 FROM iam_user_groups ug JOIN iam_group_policies gp ON gp.group_id = ug.group_id
                  WHERE ug.user_id = ?1 AND gp.policy_id = p.id)
    ORDER BY p.name";

/// Stored documents were validated on write; an invalid one fails closed.
fn stored_document(name: &str, json: &str) -> Result<PolicyDocument, AppError> {
    parse_document(json).map_err(|e| {
        tracing::error!(policy = name, %e, "stored policy document is invalid");
        AppError::Internal
    })
}

/// Policies in force for an account (one query). Status dominates attachments:
/// blocked → nothing; pending → baseline + `Publico`; active → baseline +
/// `Publico` + attachments. Never cached, so changes apply on the next request.
#[tracing::instrument(level = "debug", skip_all, err(level = "debug"))]
pub async fn principal_access(
    conn: &Connection,
    user_id: &str,
    status: AccountStatus,
) -> Result<Access, AppError> {
    let attached_user = match status {
        AccountStatus::Blocked => return Ok(Access::default()),
        AccountStatus::Pending => "",
        AccountStatus::Active => user_id,
    };
    let rows = query_all(conn, ATTACHED_SQL, (attached_user, PUBLICO)).await?;
    let mut policies = Vec::with_capacity(rows.len() + 1);
    policies.push(baseline());
    for row in rows {
        let name: String = row.get(0)?;
        let doc = stored_document(&name, &row.get::<String>(1)?)?;
        policies.push((name, doc));
    }
    Ok(Access::new(policies))
}

/// Access of anonymous visitors: the managed `Publico` policy only.
#[tracing::instrument(level = "debug", skip_all, err(level = "debug"))]
pub async fn public_access(conn: &Connection) -> Result<Access, AppError> {
    let row = query_opt(
        conn,
        "SELECT name, document FROM iam_policies WHERE id = ?1",
        [PUBLICO],
    )
    .await?
    .ok_or_else(|| AppError::internal("missing or invalid stored value"))?;
    let name: String = row.get(0)?;
    let doc = stored_document(&name, &row.get::<String>(1)?)?;
    Ok(Access::new(vec![(name, doc)]))
}

/// Public reads: a visitor is evaluated against `Publico`, an account against
/// its own access. Denied visitors are asked to sign in.
#[tracing::instrument(level = "debug", skip_all, err(level = "debug"))]
pub async fn require_viewer(
    conn: &Connection,
    viewer: Option<&Actor>,
    permission: Permission,
) -> Result<(), AppError> {
    match viewer {
        Some(actor) => actor.require(permission),
        None if public_access(conn).await?.allows(permission, ANY_RESOURCE) => Ok(()),
        None => Err(AppError::Unauthenticated),
    }
}

/// Check scoped to a jurisdiction (`jurisdiction:<IBGE code>`). The code is only
/// looked up when some attached statement names a specific resource.
#[tracing::instrument(level = "debug", skip_all, fields(actor = %actor.user_id), err(level = "debug"))]
pub async fn can_in_jurisdiction(
    conn: &Connection,
    actor: &Actor,
    permission: Permission,
    jurisdiction_id: &str,
) -> Result<bool, AppError> {
    if !actor.access.is_scoped() {
        return Ok(actor.can(permission));
    }
    let code: String = query_opt(
        conn,
        "SELECT code FROM jurisdictions WHERE id = ?1",
        [jurisdiction_id],
    )
    .await?
    .ok_or(AppError::NotFound)?
    .get(0)?;
    Ok(actor.can_on(permission, &jurisdiction_resource(&code)))
}

#[tracing::instrument(level = "debug", skip_all, fields(actor = %actor.user_id), err(level = "debug"))]
pub async fn require_in_jurisdiction(
    conn: &Connection,
    actor: &Actor,
    permission: Permission,
    jurisdiction_id: &str,
) -> Result<(), AppError> {
    if can_in_jurisdiction(conn, actor, permission, jurisdiction_id).await? {
        Ok(())
    } else {
        Err(AppError::Forbidden)
    }
}

/// Cheap pre-check before opening a write transaction: the action is allowed
/// globally or may be allowed for some jurisdiction.
pub fn may_act(actor: &Actor, permission: Permission) -> Result<(), AppError> {
    if actor.can(permission) || actor.access.is_scoped() {
        Ok(())
    } else {
        Err(AppError::Forbidden)
    }
}

/// Attaches `Cidadao` (idempotent). Called when an account becomes active.
#[tracing::instrument(level = "debug", skip_all, err(level = "debug"))]
pub async fn attach_cidadao(
    conn: &Connection,
    now: DateTime<Utc>,
    user_id: &str,
    by: &str,
) -> Result<(), AppError> {
    conn.execute(
        "INSERT INTO iam_user_policies (user_id, policy_id, attached_at, attached_by) VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(user_id, policy_id) DO NOTHING",
        (user_id, CIDADAO, ts(now), by),
    )
    .await?;
    Ok(())
}

/// True when some active AdministratorAccess holder can still manage IAM.
async fn has_iam_manager(conn: &Connection) -> Result<bool, AppError> {
    let mut holders = Vec::new();
    for sql in [
        "SELECT u.id FROM iam_user_policies up JOIN users u ON u.id = up.user_id
         WHERE up.policy_id = ?1 AND u.status = 'active'",
        "SELECT u.id FROM iam_group_policies gp JOIN iam_user_groups ug ON ug.group_id = gp.group_id
         JOIN users u ON u.id = ug.user_id WHERE gp.policy_id = ?1 AND u.status = 'active'",
    ] {
        for row in query_all(conn, sql, [ADMINISTRATOR_ACCESS]).await? {
            holders.push(row.get::<String>(0)?);
        }
    }
    holders.sort_unstable();
    holders.dedup();
    for id in &holders {
        if principal_access(conn, id, AccountStatus::Active)
            .await?
            .allows(Permission::IamManage, ANY_RESOURCE)
        {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Open IAM change: permission checked, reauth grant consumed, lockout baseline taken.
struct Change {
    tx: WriteTx,
    had_manager: bool,
}

async fn begin(db: &Db, now: DateTime<Utc>, actor: &Actor) -> Result<Change, AppError> {
    actor.require(Permission::IamManage)?;
    let tx = db.write().await?;
    reauth::consume(
        &tx,
        now,
        &actor.session_hash,
        REAUTH_IAM,
        REAUTH_IAM_RESOURCE,
    )
    .await?;
    let had_manager = has_iam_manager(&tx).await?;
    Ok(Change { tx, had_manager })
}

/// Lockout guard, audit event and commit. Any error leaves the change uncommitted.
async fn finish(
    change: Change,
    now: DateTime<Utc>,
    actor_ref: &str,
    kind: &str,
    subject: &str,
    payload: serde_json::Value,
) -> Result<(), AppError> {
    if change.had_manager && !has_iam_manager(&change.tx).await? {
        return Err(AppError::Conflict(t("iam.lockout").into()));
    }
    audit::append(
        &change.tx,
        now,
        Event {
            kind,
            visibility: Visibility::Restricted,
            actor_ref: Some(actor_ref),
            subject: Some(subject),
            payload,
        },
    )
    .await?;
    change.tx.commit().await?;
    Ok(())
}

fn clean_name(name: &str) -> Result<String, AppError> {
    let name = name.trim();
    if valid_name(name) {
        Ok(name.to_owned())
    } else {
        Err(AppError::validation("name", t("iam.name.invalid")))
    }
}

fn clean_description(description: &str) -> Result<String, AppError> {
    let description = description.trim();
    if description.chars().count() <= DESCRIPTION_MAX && !description.chars().any(char::is_control)
    {
        Ok(description.to_owned())
    } else {
        Err(AppError::validation(
            "description",
            t("iam.description.invalid"),
        ))
    }
}

/// Validates and minifies a document for storage.
fn clean_document(document: &str) -> Result<String, AppError> {
    parse_document(document).map_err(|e| AppError::validation("document", e.user_message()))?;
    let value: serde_json::Value = serde_json::from_str(document).map_err(AppError::internal)?;
    Ok(value.to_string())
}

struct PolicyRow {
    name: String,
    managed: bool,
}

async fn policy_row(conn: &Connection, id: &str) -> Result<PolicyRow, AppError> {
    let row = query_opt(
        conn,
        "SELECT name, managed FROM iam_policies WHERE id = ?1",
        [id],
    )
    .await?
    .ok_or(AppError::NotFound)?;
    Ok(PolicyRow {
        name: row.get(0)?,
        managed: row.get::<i64>(1)? == 1,
    })
}

async fn group_name(conn: &Connection, id: &str) -> Result<String, AppError> {
    Ok(
        query_opt(conn, "SELECT name FROM iam_groups WHERE id = ?1", [id])
            .await?
            .ok_or(AppError::NotFound)?
            .get(0)?,
    )
}

async fn user_ref(conn: &Connection, id: &str) -> Result<UserRef, AppError> {
    let row = query_opt(
        conn,
        "SELECT id, email, name, status FROM users WHERE id = ?1",
        [id],
    )
    .await?
    .ok_or(AppError::NotFound)?;
    Ok(UserRef {
        id: row.get(0)?,
        email: row.get(1)?,
        name: row.get(2)?,
        status: row.get(3)?,
    })
}

async fn user_id_by_email(conn: &Connection, email: &str) -> Result<String, AppError> {
    let email = normalize_email(email).ok_or_else(user_not_found)?;
    Ok(
        query_opt(conn, "SELECT id FROM users WHERE email = ?1", [email])
            .await?
            .ok_or_else(user_not_found)?
            .get(0)?,
    )
}

fn user_not_found() -> AppError {
    AppError::validation("email", t("iam.user.not_found"))
}

fn constraint_conflict(error: turso::Error, message: &'static str) -> AppError {
    if is_constraint(&error) {
        AppError::Conflict(t(message).into())
    } else {
        error.into()
    }
}

// ---- Policies ----

#[tracing::instrument(level = "debug", skip_all, fields(actor = %actor.user_id), err(level = "debug"))]
pub async fn list_policies(
    conn: &Connection,
    actor: &Actor,
) -> Result<Vec<PolicySummary>, AppError> {
    actor.require(Permission::IamRead)?;
    let rows = query_all(
        conn,
        "SELECT p.id, p.name, p.description, p.managed, p.version,
                (SELECT COUNT(*) FROM iam_user_policies up WHERE up.policy_id = p.id)
              + (SELECT COUNT(*) FROM iam_group_policies gp WHERE gp.policy_id = p.id)
         FROM iam_policies p ORDER BY p.managed DESC, p.name",
        (),
    )
    .await?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        out.push(PolicySummary {
            id: row.get(0)?,
            name: row.get(1)?,
            description: row.get(2)?,
            managed: row.get::<i64>(3)? == 1,
            version: row.get(4)?,
            attachments: row.get(5)?,
        });
    }
    Ok(out)
}

#[tracing::instrument(level = "debug", skip_all, fields(actor = %actor.user_id), err(level = "debug"))]
pub async fn get_policy(
    conn: &Connection,
    actor: &Actor,
    id: &str,
) -> Result<PolicyView, AppError> {
    actor.require(Permission::IamRead)?;
    let row = query_opt(
        conn,
        "SELECT id, name, description, document, managed, version, updated_at FROM iam_policies WHERE id = ?1",
        [id],
    )
    .await?
    .ok_or(AppError::NotFound)?;
    let document: String = row.get(3)?;
    let pretty = serde_json::from_str::<serde_json::Value>(&document)
        .and_then(|v| serde_json::to_string_pretty(&v))
        .unwrap_or(document);
    Ok(PolicyView {
        id: row.get(0)?,
        name: row.get(1)?,
        description: row.get(2)?,
        document: pretty,
        managed: row.get::<i64>(4)? == 1,
        version: row.get(5)?,
        updated_at: row.get(6)?,
    })
}

#[tracing::instrument(level = "debug", skip_all, fields(actor = %actor.user_id), err(level = "debug"))]
pub async fn create_policy(
    db: &Db,
    now: DateTime<Utc>,
    actor: &Actor,
    name: &str,
    description: &str,
    document: &str,
) -> Result<String, AppError> {
    actor.require(Permission::IamManage)?;
    let name = clean_name(name)?;
    let description = clean_description(description)?;
    let document = clean_document(document)?;
    let change = begin(db, now, actor).await?;
    let id = new_id();
    change
        .tx
        .execute(
            "INSERT INTO iam_policies (id, name, description, document, managed, version, created_at, updated_at, created_by)
             VALUES (?1, ?2, ?3, ?4, 0, 1, ?5, ?5, ?6)",
            (id.as_str(), name.as_str(), description.as_str(), document.as_str(), ts(now), actor.user_id.as_str()),
        )
        .await
        .map_err(|e| constraint_conflict(e, "iam.name.taken"))?;
    finish(
        change,
        now,
        &actor.user_id,
        "iam.policy_created",
        &id,
        json!({ "policy": name, "version": 1 }),
    )
    .await?;
    Ok(id)
}

#[tracing::instrument(level = "debug", skip_all, fields(actor = %actor.user_id), err(level = "debug"))]
pub async fn update_policy(
    db: &Db,
    now: DateTime<Utc>,
    actor: &Actor,
    id: &str,
    description: &str,
    document: &str,
) -> Result<(), AppError> {
    actor.require(Permission::IamManage)?;
    let description = clean_description(description)?;
    let document = clean_document(document)?;
    let change = begin(db, now, actor).await?;
    let policy = policy_row(&change.tx, id).await?;
    if policy.managed {
        return Err(AppError::Conflict(t("iam.policy.managed").into()));
    }
    change
        .tx
        .execute(
            "UPDATE iam_policies SET description = ?2, document = ?3, version = version + 1, updated_at = ?4 WHERE id = ?1",
            (id, description.as_str(), document.as_str(), ts(now)),
        )
        .await?;
    let version: i64 = query_opt(
        &change.tx,
        "SELECT version FROM iam_policies WHERE id = ?1",
        [id],
    )
    .await?
    .ok_or_else(|| AppError::internal("missing or invalid stored value"))?
    .get(0)?;
    finish(
        change,
        now,
        &actor.user_id,
        "iam.policy_updated",
        id,
        json!({ "policy": policy.name, "version": version }),
    )
    .await
}

/// Customer policies only, and only once detached everywhere (as in IAM).
#[tracing::instrument(level = "debug", skip_all, fields(actor = %actor.user_id), err(level = "debug"))]
pub async fn delete_policy(
    db: &Db,
    now: DateTime<Utc>,
    actor: &Actor,
    id: &str,
) -> Result<(), AppError> {
    let change = begin(db, now, actor).await?;
    let policy = policy_row(&change.tx, id).await?;
    if policy.managed {
        return Err(AppError::Conflict(t("iam.policy.managed").into()));
    }
    let attached = query_opt(
        &change.tx,
        "SELECT 1 FROM iam_user_policies WHERE policy_id = ?1
         UNION ALL SELECT 1 FROM iam_group_policies WHERE policy_id = ?1 LIMIT 1",
        [id],
    )
    .await?
    .is_some();
    if attached {
        return Err(AppError::Conflict(t("iam.policy.attached").into()));
    }
    change
        .tx
        .execute("DELETE FROM iam_policies WHERE id = ?1", [id])
        .await?;
    finish(
        change,
        now,
        &actor.user_id,
        "iam.policy_deleted",
        id,
        json!({ "policy": policy.name }),
    )
    .await
}

// ---- Groups ----

#[tracing::instrument(level = "debug", skip_all, fields(actor = %actor.user_id), err(level = "debug"))]
pub async fn list_groups(conn: &Connection, actor: &Actor) -> Result<Vec<GroupSummary>, AppError> {
    actor.require(Permission::IamRead)?;
    let rows = query_all(
        conn,
        "SELECT g.id, g.name, g.description,
                (SELECT COUNT(*) FROM iam_user_groups ug WHERE ug.group_id = g.id),
                (SELECT COUNT(*) FROM iam_group_policies gp WHERE gp.group_id = g.id)
         FROM iam_groups g ORDER BY g.name",
        (),
    )
    .await?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        out.push(GroupSummary {
            id: row.get(0)?,
            name: row.get(1)?,
            description: row.get(2)?,
            members: row.get(3)?,
            policies: row.get(4)?,
        });
    }
    Ok(out)
}

#[tracing::instrument(level = "debug", skip_all, fields(actor = %actor.user_id), err(level = "debug"))]
pub async fn get_group(conn: &Connection, actor: &Actor, id: &str) -> Result<GroupView, AppError> {
    actor.require(Permission::IamRead)?;
    let row = query_opt(
        conn,
        "SELECT id, name, description FROM iam_groups WHERE id = ?1",
        [id],
    )
    .await?
    .ok_or(AppError::NotFound)?;
    let policies = policy_refs(
        conn,
        "SELECT p.id, p.name, p.managed FROM iam_group_policies gp JOIN iam_policies p ON p.id = gp.policy_id
         WHERE gp.group_id = ?1 ORDER BY p.name",
        id,
    )
    .await?;
    let rows = query_all(
        conn,
        "SELECT u.id, u.email, u.name, u.status FROM iam_user_groups ug JOIN users u ON u.id = ug.user_id
         WHERE ug.group_id = ?1 ORDER BY u.email",
        [id],
    )
    .await?;
    let mut members = Vec::with_capacity(rows.len());
    for r in rows {
        members.push(UserRef {
            id: r.get(0)?,
            email: r.get(1)?,
            name: r.get(2)?,
            status: r.get(3)?,
        });
    }
    Ok(GroupView {
        id: row.get(0)?,
        name: row.get(1)?,
        description: row.get(2)?,
        policies,
        members,
    })
}

async fn policy_refs(conn: &Connection, sql: &str, key: &str) -> Result<Vec<PolicyRef>, AppError> {
    let rows = query_all(conn, sql, [key]).await?;
    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        out.push(PolicyRef {
            id: r.get(0)?,
            name: r.get(1)?,
            managed: r.get::<i64>(2)? == 1,
        });
    }
    Ok(out)
}

#[tracing::instrument(level = "debug", skip_all, fields(actor = %actor.user_id), err(level = "debug"))]
pub async fn create_group(
    db: &Db,
    now: DateTime<Utc>,
    actor: &Actor,
    name: &str,
    description: &str,
) -> Result<String, AppError> {
    actor.require(Permission::IamManage)?;
    let name = clean_name(name)?;
    let description = clean_description(description)?;
    let change = begin(db, now, actor).await?;
    let id = new_id();
    change
        .tx
        .execute(
            "INSERT INTO iam_groups (id, name, description, created_at, created_by) VALUES (?1, ?2, ?3, ?4, ?5)",
            (id.as_str(), name.as_str(), description.as_str(), ts(now), actor.user_id.as_str()),
        )
        .await
        .map_err(|e| constraint_conflict(e, "iam.name.taken"))?;
    finish(
        change,
        now,
        &actor.user_id,
        "iam.group_created",
        &id,
        json!({ "group": name }),
    )
    .await?;
    Ok(id)
}

#[tracing::instrument(level = "debug", skip_all, fields(actor = %actor.user_id), err(level = "debug"))]
pub async fn delete_group(
    db: &Db,
    now: DateTime<Utc>,
    actor: &Actor,
    id: &str,
) -> Result<(), AppError> {
    let change = begin(db, now, actor).await?;
    let name = group_name(&change.tx, id).await?;
    for sql in [
        "DELETE FROM iam_user_groups WHERE group_id = ?1",
        "DELETE FROM iam_group_policies WHERE group_id = ?1",
        "DELETE FROM iam_groups WHERE id = ?1",
    ] {
        change.tx.execute(sql, [id]).await?;
    }
    finish(
        change,
        now,
        &actor.user_id,
        "iam.group_deleted",
        id,
        json!({ "group": name }),
    )
    .await
}

#[tracing::instrument(level = "debug", skip_all, fields(actor = %actor.user_id), err(level = "debug"))]
pub async fn set_group_policy(
    db: &Db,
    now: DateTime<Utc>,
    actor: &Actor,
    group_id: &str,
    policy_id: &str,
    attach: bool,
) -> Result<(), AppError> {
    let change = begin(db, now, actor).await?;
    let group = group_name(&change.tx, group_id).await?;
    let policy = policy_row(&change.tx, policy_id).await?;
    let changed = if attach {
        change
            .tx
            .execute(
                "INSERT INTO iam_group_policies (group_id, policy_id, attached_at, attached_by) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(group_id, policy_id) DO NOTHING",
                (group_id, policy_id, ts(now), actor.user_id.as_str()),
            )
            .await?
    } else {
        change
            .tx
            .execute(
                "DELETE FROM iam_group_policies WHERE group_id = ?1 AND policy_id = ?2",
                (group_id, policy_id),
            )
            .await?
    };
    unchanged_conflict(changed, attach)?;
    let kind = if attach {
        "iam.group_policy_attached"
    } else {
        "iam.group_policy_detached"
    };
    finish(
        change,
        now,
        &actor.user_id,
        kind,
        group_id,
        json!({ "group": group, "policy": policy.name }),
    )
    .await
}

/// Adds or removes a member, identified by e-mail.
#[tracing::instrument(level = "debug", skip_all, fields(actor = %actor.user_id), err(level = "debug"))]
pub async fn set_group_member(
    db: &Db,
    now: DateTime<Utc>,
    actor: &Actor,
    group_id: &str,
    email: &str,
    member: bool,
) -> Result<(), AppError> {
    let change = begin(db, now, actor).await?;
    let group = group_name(&change.tx, group_id).await?;
    let user_id = user_id_by_email(&change.tx, email).await?;
    let changed = if member {
        change
            .tx
            .execute(
                "INSERT INTO iam_user_groups (user_id, group_id, added_at, added_by) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(user_id, group_id) DO NOTHING",
                (user_id.as_str(), group_id, ts(now), actor.user_id.as_str()),
            )
            .await?
    } else {
        change
            .tx
            .execute(
                "DELETE FROM iam_user_groups WHERE user_id = ?1 AND group_id = ?2",
                (user_id.as_str(), group_id),
            )
            .await?
    };
    unchanged_conflict(changed, member)?;
    let kind = if member {
        "iam.group_member_added"
    } else {
        "iam.group_member_removed"
    };
    finish(
        change,
        now,
        &actor.user_id,
        kind,
        &user_id,
        json!({ "group": group }),
    )
    .await
}

fn unchanged_conflict(changed: u64, adding: bool) -> Result<(), AppError> {
    match (changed, adding) {
        (0, true) => Err(AppError::Conflict(t("iam.already_attached").into())),
        (0, false) => Err(AppError::NotFound),
        _ => Ok(()),
    }
}

// ---- Users ----

/// Accounts whose e-mail starts with `query` (2+ characters), at most 20.
#[tracing::instrument(level = "debug", skip_all, fields(actor = %actor.user_id), err(level = "debug"))]
pub async fn search_users(
    conn: &Connection,
    actor: &Actor,
    query: &str,
) -> Result<Vec<UserRef>, AppError> {
    actor.require(Permission::IamRead)?;
    let prefix = query.trim().to_lowercase();
    if !(2..=254).contains(&prefix.len()) || prefix.chars().any(char::is_control) {
        return Err(AppError::validation("query", t("iam.user.query_invalid")));
    }
    let upper = format!("{prefix}\u{10FFFF}");
    let rows = query_all(
        conn,
        "SELECT id, email, name, status FROM users WHERE email >= ?1 AND email < ?2 ORDER BY email LIMIT 20",
        (prefix.as_str(), upper.as_str()),
    )
    .await?;
    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        out.push(UserRef {
            id: r.get(0)?,
            email: r.get(1)?,
            name: r.get(2)?,
            status: r.get(3)?,
        });
    }
    Ok(out)
}

fn status_of(user: &UserRef) -> Result<AccountStatus, AppError> {
    AccountStatus::from_db(&user.status)
        .ok_or_else(|| AppError::internal("missing or invalid stored value"))
}

#[tracing::instrument(level = "debug", skip_all, fields(actor = %actor.user_id), err(level = "debug"))]
pub async fn user_detail(conn: &Connection, actor: &Actor, id: &str) -> Result<UserView, AppError> {
    actor.require(Permission::IamRead)?;
    let user = user_ref(conn, id).await?;
    let policies = policy_refs(
        conn,
        "SELECT p.id, p.name, p.managed FROM iam_user_policies up JOIN iam_policies p ON p.id = up.policy_id
         WHERE up.user_id = ?1 ORDER BY p.name",
        id,
    )
    .await?;
    let rows = query_all(
        conn,
        "SELECT g.id, g.name FROM iam_user_groups ug JOIN iam_groups g ON g.id = ug.group_id
         WHERE ug.user_id = ?1 ORDER BY g.name",
        [id],
    )
    .await?;
    let mut groups = Vec::with_capacity(rows.len());
    for r in rows {
        groups.push(GroupRef {
            id: r.get(0)?,
            name: r.get(1)?,
        });
    }
    let status = status_of(&user)?;
    let allowed = principal_access(conn, id, status)
        .await?
        .allowed_actions()
        .into_iter()
        .map(|p| p.as_str().to_owned())
        .collect();
    Ok(UserView {
        user,
        policies,
        groups,
        allowed,
    })
}

#[tracing::instrument(level = "debug", skip_all, fields(actor = %actor.user_id), err(level = "debug"))]
pub async fn set_user_policy(
    db: &Db,
    now: DateTime<Utc>,
    actor: &Actor,
    user_id: &str,
    policy_id: &str,
    attach: bool,
) -> Result<(), AppError> {
    let change = begin(db, now, actor).await?;
    user_ref(&change.tx, user_id).await?;
    let policy = policy_row(&change.tx, policy_id).await?;
    let changed = if attach {
        change
            .tx
            .execute(
                "INSERT INTO iam_user_policies (user_id, policy_id, attached_at, attached_by) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(user_id, policy_id) DO NOTHING",
                (user_id, policy_id, ts(now), actor.user_id.as_str()),
            )
            .await?
    } else {
        change
            .tx
            .execute(
                "DELETE FROM iam_user_policies WHERE user_id = ?1 AND policy_id = ?2",
                (user_id, policy_id),
            )
            .await?
    };
    unchanged_conflict(changed, attach)?;
    let kind = if attach {
        "iam.user_policy_attached"
    } else {
        "iam.user_policy_detached"
    };
    finish(
        change,
        now,
        &actor.user_id,
        kind,
        user_id,
        json!({ "policy": policy.name }),
    )
    .await
}

/// Request resource accepted by the simulator: `*` or `jurisdiction:<code>`.
fn valid_request_resource(resource: &str) -> bool {
    resource == ANY_RESOURCE
        || resource.strip_prefix("jurisdiction:").is_some_and(|code| {
            (1..=16).contains(&code.len()) && code.chars().all(|c| c.is_ascii_alphanumeric())
        })
}

/// Evaluates `action` on `resource` for the account with this e-mail, exactly as
/// a request would (status, `Publico`, attachments and groups).
#[tracing::instrument(level = "debug", skip_all, fields(actor = %actor.user_id), err(level = "debug"))]
pub async fn simulate(
    conn: &Connection,
    actor: &Actor,
    email: &str,
    action: &str,
    resource: &str,
) -> Result<Simulation, AppError> {
    actor.require(Permission::IamRead)?;
    let permission = Permission::parse(action.trim())
        .ok_or_else(|| AppError::validation("action", t("iam.doc.unknown_action")))?;
    let resource = resource.trim();
    let resource = if resource.is_empty() {
        ANY_RESOURCE
    } else {
        resource
    };
    if !valid_request_resource(resource) {
        return Err(AppError::validation("resource", t("iam.doc.resource")));
    }
    let user_id = user_id_by_email(conn, email).await?;
    let user = user_ref(conn, &user_id).await?;
    let status = status_of(&user)?;
    let decision = principal_access(conn, &user_id, status)
        .await?
        .evaluate(permission, resource);
    Ok(Simulation {
        user,
        action: permission.as_str().to_owned(),
        resource: resource.to_owned(),
        decision,
    })
}

/// Operator bootstrap from the VPS shell (`dwc grant-admin <email>`): attaches
/// AdministratorAccess to an existing active account. Not exposed over HTTP.
#[tracing::instrument(level = "debug", skip_all, err(level = "debug"))]
pub async fn grant_administrator(db: &Db, now: DateTime<Utc>, email: &str) -> Result<(), AppError> {
    let email = normalize_email(email).ok_or_else(user_not_found)?;
    let tx = db.write().await?;
    let row = query_opt(
        &tx,
        "SELECT id, status FROM users WHERE email = ?1",
        [email.as_str()],
    )
    .await?
    .ok_or_else(user_not_found)?;
    let user_id: String = row.get(0)?;
    if row.get::<String>(1)? != AccountStatus::Active.as_db() {
        return Err(user_not_found());
    }
    let added = tx
        .execute(
            "INSERT INTO iam_user_policies (user_id, policy_id, attached_at, attached_by) VALUES (?1, ?2, ?3, 'operator:cli')
             ON CONFLICT(user_id, policy_id) DO NOTHING",
            (user_id.as_str(), ADMINISTRATOR_ACCESS, ts(now)),
        )
        .await?;
    unchanged_conflict(added, true)?;
    audit::append(
        &tx,
        now,
        Event {
            kind: "iam.user_policy_attached",
            visibility: Visibility::Restricted,
            actor_ref: Some("operator:cli"),
            subject: Some(&user_id),
            payload: json!({ "policy": "AdministratorAccess" }),
        },
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        application::{
            actor,
            audit::{ChainStatus, verify},
            session::{
                SessionPolicy,
                testing::{account_with_session, attach_policy},
                token_hash,
            },
        },
        domain::iam::MODERADOR,
        infrastructure::{
            catalog,
            db::testing::{TestDb, migrated},
        },
    };

    const MINI: &str = "code,kind,name,parent_code,state_abbrev\nBR,national,Brasil,,\n42,state,SC,BR,SC\n4202404,municipal,Blumenau,42,SC\n";
    const MODERATION: &str = r#"{"Version":"2026-10-25","Statement":[{"Sid":"Moderar","Effect":"Allow","Action":"moderation:act","Resource":"*"}]}"#;
    const DENY_ALL: &str = r#"{"Version":"2026-10-25","Statement":[{"Sid":"Bloqueio","Effect":"Deny","Action":"*","Resource":"*"}]}"#;
    const DENY_VOTE: &str = r#"{"Version":"2026-10-25","Statement":[{"Sid":"NaoVota","Effect":"Deny","Action":"proposal:vote","Resource":"*"}]}"#;

    fn now() -> DateTime<Utc> {
        "2026-10-25T12:00:00Z".parse().unwrap()
    }

    async fn world() -> TestDb {
        let t = migrated().await;
        catalog::import_str(&t.db, "mini", MINI).await.unwrap();
        t
    }

    /// Account with the given status and policies; returns the loaded actor.
    async fn account(t: &TestDb, status: &str, policies: &[&str]) -> Actor {
        let (user, token) = account_with_session(
            &t.db,
            now(),
            SessionPolicy::from_secs(900, 28800),
            "4202404",
            status,
            policies,
        )
        .await;
        reload(t, &user, &token_hash(&token)).await
    }

    async fn reload(t: &TestDb, user_id: &str, session_hash: &str) -> Actor {
        actor::load(&t.db.read().await.unwrap(), user_id, session_hash)
            .await
            .unwrap()
            .unwrap()
    }

    async fn fresh(t: &TestDb, a: &Actor) -> Actor {
        reload(t, &a.user_id, &a.session_hash).await
    }

    async fn email(t: &TestDb, a: &Actor) -> String {
        user_ref(&t.db.read().await.unwrap(), &a.user_id)
            .await
            .unwrap()
            .email
    }

    /// Face + TOTP verified: one `iam:manage` grant for the actor's session.
    async fn grant(t: &TestDb, a: &Actor) {
        let tx = t.db.write().await.unwrap();
        reauth::grant(&tx, now(), &a.session_hash, REAUTH_IAM, REAUTH_IAM_RESOURCE)
            .await
            .unwrap();
        tx.commit().await.unwrap();
    }

    async fn iam_events(t: &TestDb) -> Vec<(String, String, String)> {
        query_all(
            &t.db.read().await.unwrap(),
            "SELECT kind, visibility, actor_ref FROM audit_events WHERE kind LIKE 'iam.%' ORDER BY seq",
            (),
        )
        .await
        .unwrap()
        .iter()
        .map(|r| (r.get(0).unwrap(), r.get(1).unwrap(), r.get(2).unwrap()))
        .collect()
    }

    #[tokio::test]
    async fn account_status_dominates_attached_policies() {
        let t = world().await;
        let citizen = account(&t, "active", &["Cidadao"]).await;
        assert!(citizen.can(Permission::ProposalVote));
        assert!(citizen.can(Permission::ProfileUpdate));
        assert!(!citizen.can(Permission::ModerationAct));
        assert!(!citizen.can(Permission::IamRead));

        let pending = account(&t, "pending", &["Cidadao", "AdministratorAccess"]).await;
        assert_eq!(
            pending.access.allowed_actions(),
            vec![
                Permission::ProfileRead,
                Permission::ProposalRead,
                Permission::AuditRead,
                Permission::StatsRead
            ],
            "pending: own profile + public reads only"
        );
        let conn = t.db.read().await.unwrap();
        let blocked = principal_access(&conn, &citizen.user_id, AccountStatus::Blocked)
            .await
            .unwrap();
        assert!(blocked.allowed_actions().is_empty());

        // An active account without policies keeps only its profile and public reads.
        let bare = account(&t, "active", &[]).await;
        assert!(bare.can(Permission::ProfileRead) && bare.can(Permission::ProposalRead));
        assert!(!bare.can(Permission::ProposalCreate));
    }

    #[tokio::test]
    async fn visitors_get_publico_only() {
        let t = world().await;
        let conn = t.db.read().await.unwrap();
        for p in [
            Permission::ProposalRead,
            Permission::AuditRead,
            Permission::StatsRead,
        ] {
            assert_eq!(require_viewer(&conn, None, p).await, Ok(()));
        }
        for p in [
            Permission::ProposalVote,
            Permission::ProfileRead,
            Permission::IamRead,
        ] {
            assert_eq!(
                require_viewer(&conn, None, p).await,
                Err(AppError::Unauthenticated)
            );
        }
        let citizen = account(&t, "active", &["Cidadao"]).await;
        assert_eq!(
            require_viewer(&conn, Some(&citizen), Permission::IamRead).await,
            Err(AppError::Forbidden)
        );
    }

    #[tokio::test]
    async fn changes_require_iam_manage_and_a_fresh_reauth_and_are_audited() {
        let t = world().await;
        let citizen = account(&t, "active", &["Cidadao"]).await;
        grant(&t, &citizen).await;
        assert_eq!(
            create_policy(&t.db, now(), &citizen, "Moderacao", "", MODERATION).await,
            Err(AppError::Forbidden)
        );
        let conn = t.db.read().await.unwrap();
        assert_eq!(
            list_policies(&conn, &citizen).await,
            Err(AppError::Forbidden)
        );

        let master = account(&t, "active", &["AdministratorAccess", "Cidadao"]).await;
        assert_eq!(
            create_policy(&t.db, now(), &master, "Moderacao", "", MODERATION).await,
            Err(AppError::ReauthRequired)
        );
        grant(&t, &master).await;
        let id = create_policy(&t.db, now(), &master, "Moderacao", "Fila", MODERATION)
            .await
            .unwrap();
        assert_eq!(
            create_policy(&t.db, now(), &master, "Outra", "", MODERATION).await,
            Err(AppError::ReauthRequired),
            "grant is single use"
        );
        let view = get_policy(&conn, &master, &id).await.unwrap();
        assert!(!view.managed && view.version == 1 && view.document.contains("moderation:act"));
        let listed = list_policies(&conn, &master).await.unwrap();
        assert_eq!(listed.len(), 6);
        assert!(listed[..5].iter().all(|p| p.managed), "managed first");
        assert_eq!(
            iam_events(&t).await,
            vec![(
                "iam.policy_created".into(),
                "restricted".into(),
                master.user_id.clone()
            )]
        );
        assert!(matches!(
            verify(&conn).await.unwrap(),
            ChainStatus::Valid { .. }
        ));
    }

    #[tokio::test]
    async fn documents_are_validated_and_managed_policies_are_read_only() {
        let t = world().await;
        let master = account(&t, "active", &["AdministratorAccess"]).await;
        grant(&t, &master).await;
        let bad = r#"{"Version":"2026-10-25","Statement":[{"Effect":"Allow","Action":"proposal:delete","Resource":"*"}]}"#;
        let Err(AppError::Validation { field, message }) =
            create_policy(&t.db, now(), &master, "Ruim", "", bad).await
        else {
            panic!("unknown action accepted");
        };
        assert_eq!(field, "document");
        assert!(message.contains("proposal:delete"), "{message}");
        assert!(matches!(
            create_policy(&t.db, now(), &master, "com espaço", "", MODERATION).await,
            Err(AppError::Validation { .. })
        ));
        assert!(matches!(
            create_policy(&t.db, now(), &master, "Cidadao", "", MODERATION).await,
            Err(AppError::Conflict(_))
        ));
        for managed in [CIDADAO, ADMINISTRATOR_ACCESS, PUBLICO] {
            grant(&t, &master).await;
            assert!(matches!(
                update_policy(&t.db, now(), &master, managed, "", DENY_ALL).await,
                Err(AppError::Conflict(_))
            ));
            assert!(matches!(
                delete_policy(&t.db, now(), &master, managed).await,
                Err(AppError::Conflict(_))
            ));
        }
        // The database itself refuses to change managed documents.
        let tx = t.db.write().await.unwrap();
        assert!(
            tx.execute(
                "UPDATE iam_policies SET document = '{}' WHERE id = ?1",
                [CIDADAO]
            )
            .await
            .is_err()
        );
        drop(tx);

        let id = create_policy(&t.db, now(), &master, "Moderacao", "", MODERATION)
            .await
            .unwrap();
        grant(&t, &master).await;
        update_policy(&t.db, now(), &master, &id, "v2", DENY_ALL)
            .await
            .unwrap();
        let conn = t.db.read().await.unwrap();
        let v = get_policy(&conn, &master, &id).await.unwrap();
        assert_eq!((v.version, v.description.as_str()), (2, "v2"));
        assert!(
            iam_events(&t)
                .await
                .iter()
                .any(|e| e.0 == "iam.policy_updated")
        );
    }

    #[tokio::test]
    async fn attachments_apply_on_the_next_request_and_attached_policies_cannot_be_deleted() {
        let t = world().await;
        let master = account(&t, "active", &["AdministratorAccess"]).await;
        let ana = account(&t, "active", &["Cidadao"]).await;
        grant(&t, &master).await;
        let id = create_policy(&t.db, now(), &master, "Moderacao", "", MODERATION)
            .await
            .unwrap();
        grant(&t, &master).await;
        set_user_policy(&t.db, now(), &master, &ana.user_id, &id, true)
            .await
            .unwrap();
        assert!(
            !ana.can(Permission::ModerationAct),
            "loaded before the change"
        );
        assert!(fresh(&t, &ana).await.can(Permission::ModerationAct));
        let conn = t.db.read().await.unwrap();
        let detail = user_detail(&conn, &master, &ana.user_id).await.unwrap();
        assert_eq!(detail.policies.len(), 2);
        assert!(detail.allowed.iter().any(|a| a == "moderation:act"));

        grant(&t, &master).await;
        assert!(matches!(
            delete_policy(&t.db, now(), &master, &id).await,
            Err(AppError::Conflict(_))
        ));
        assert!(matches!(
            set_user_policy(&t.db, now(), &master, &ana.user_id, &id, true).await,
            Err(AppError::Conflict(_))
        ));
        set_user_policy(&t.db, now(), &master, &ana.user_id, &id, false)
            .await
            .unwrap();
        assert!(!fresh(&t, &ana).await.can(Permission::ModerationAct));
        grant(&t, &master).await;
        delete_policy(&t.db, now(), &master, &id).await.unwrap();
        let kinds: Vec<String> = iam_events(&t).await.into_iter().map(|e| e.0).collect();
        assert_eq!(
            kinds,
            [
                "iam.policy_created",
                "iam.user_policy_attached",
                "iam.user_policy_detached",
                "iam.policy_deleted"
            ]
        );
    }

    #[tokio::test]
    async fn groups_grant_their_policies_to_members() {
        let t = world().await;
        let master = account(&t, "active", &["AdministratorAccess"]).await;
        let ana = account(&t, "active", &["Cidadao"]).await;
        let ana_email = email(&t, &ana).await;
        grant(&t, &master).await;
        let group = create_group(&t.db, now(), &master, "Moderadores", "Equipe")
            .await
            .unwrap();
        grant(&t, &master).await;
        set_group_policy(&t.db, now(), &master, &group, MODERADOR, true)
            .await
            .unwrap();
        grant(&t, &master).await;
        set_group_member(
            &t.db,
            now(),
            &master,
            &group,
            &ana_email.to_uppercase(),
            true,
        )
        .await
        .unwrap();
        assert!(fresh(&t, &ana).await.can(Permission::ModerationAct));
        let conn = t.db.read().await.unwrap();
        let view = get_group(&conn, &master, &group).await.unwrap();
        assert_eq!(view.policies[0].name, "Moderador");
        assert_eq!(view.members[0].email, ana_email);
        assert_eq!(list_groups(&conn, &master).await.unwrap()[0].members, 1);
        let found = search_users(&conn, &master, &ana_email[..6]).await.unwrap();
        assert!(found.iter().any(|u| u.id == ana.user_id));
        assert!(matches!(
            search_users(&conn, &master, "a").await,
            Err(AppError::Validation { .. })
        ));

        grant(&t, &master).await;
        set_group_member(&t.db, now(), &master, &group, &ana_email, false)
            .await
            .unwrap();
        assert!(!fresh(&t, &ana).await.can(Permission::ModerationAct));
        grant(&t, &master).await;
        delete_group(&t.db, now(), &master, &group).await.unwrap();
        assert!(list_groups(&conn, &master).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn last_administrator_cannot_be_removed_or_locked_out() {
        let t = world().await;
        let master = account(&t, "active", &["AdministratorAccess"]).await;
        grant(&t, &master).await;
        assert!(matches!(
            set_user_policy(
                &t.db,
                now(),
                &master,
                &master.user_id,
                ADMINISTRATOR_ACCESS,
                false
            )
            .await,
            Err(AppError::Conflict(_))
        ));
        // Rolled back: still administrator, and the grant was not spent.
        assert!(fresh(&t, &master).await.can(Permission::IamManage));
        let deny = create_policy(&t.db, now(), &master, "NegarTudo", "", DENY_ALL)
            .await
            .unwrap();
        grant(&t, &master).await;
        assert!(matches!(
            set_user_policy(&t.db, now(), &master, &master.user_id, &deny, true).await,
            Err(AppError::Conflict(_))
        ));
        // Through a group too.
        let group = create_group(&t.db, now(), &master, "Bloqueados", "")
            .await
            .unwrap();
        grant(&t, &master).await;
        set_group_policy(&t.db, now(), &master, &group, &deny, true)
            .await
            .unwrap();
        grant(&t, &master).await;
        let master_email = email(&t, &master).await;
        assert!(matches!(
            set_group_member(&t.db, now(), &master, &group, &master_email, true).await,
            Err(AppError::Conflict(_))
        ));

        // With a second administrator the first one may step down.
        let second = account(&t, "active", &["AdministratorAccess"]).await;
        set_user_policy(
            &t.db,
            now(),
            &master,
            &master.user_id,
            ADMINISTRATOR_ACCESS,
            false,
        )
        .await
        .unwrap();
        assert!(!fresh(&t, &master).await.can(Permission::IamManage));
        assert!(second.can(Permission::IamManage));
        assert!(
            iam_events(&t)
                .await
                .iter()
                .all(|e| e.1 == "restricted" && e.2 == master.user_id)
        );
    }

    #[tokio::test]
    async fn simulator_reports_the_deciding_statement() {
        let t = world().await;
        let master = account(&t, "active", &["AdministratorAccess"]).await;
        let ana = account(&t, "active", &["Cidadao"]).await;
        let ana_email = email(&t, &ana).await;
        let conn = t.db.read().await.unwrap();
        let s = simulate(&conn, &master, &ana_email, "proposal:vote", "")
            .await
            .unwrap();
        assert!(s.decision.allowed);
        assert_eq!(s.resource, "*");
        assert_eq!(s.decision.policy.as_deref(), Some("Cidadao"));
        assert_eq!(s.decision.sid.as_deref(), Some("Participacao"));
        let s = simulate(
            &conn,
            &master,
            &ana_email,
            "flags:manage",
            "jurisdiction:4202404",
        )
        .await
        .unwrap();
        assert!(!s.decision.allowed && s.decision.effect.is_none());

        grant(&t, &master).await;
        let deny = create_policy(&t.db, now(), &master, "SemVoto", "", DENY_VOTE)
            .await
            .unwrap();
        grant(&t, &master).await;
        set_user_policy(&t.db, now(), &master, &ana.user_id, &deny, true)
            .await
            .unwrap();
        let s = simulate(&conn, &master, &ana_email, "proposal:vote", "*")
            .await
            .unwrap();
        assert!(!s.decision.allowed);
        assert_eq!(s.decision.effect, Some(Effect::Deny));
        assert_eq!(s.decision.policy.as_deref(), Some("SemVoto"));
        assert_eq!(s.decision.sid.as_deref(), Some("NaoVota"));
        assert!(!fresh(&t, &ana).await.can(Permission::ProposalVote));

        assert!(matches!(
            simulate(&conn, &master, &ana_email, "proposal:delete", "*").await,
            Err(AppError::Validation { .. })
        ));
        assert!(matches!(
            simulate(&conn, &master, &ana_email, "proposal:vote", "arn:*").await,
            Err(AppError::Validation { .. })
        ));
        assert_eq!(
            simulate(&conn, &ana, &ana_email, "proposal:vote", "*").await,
            Err(AppError::Forbidden)
        );
    }

    #[tokio::test]
    async fn jurisdiction_scoped_policies_only_apply_inside_their_territory() {
        let t = world().await;
        let scoped = account(&t, "active", &["Cidadao"]).await;
        let tx = t.db.write().await.unwrap();
        tx.execute(
            "INSERT INTO iam_policies (id, name, document, created_at, updated_at) VALUES ('p-sc', 'ModeracaoSC', ?1, 't', 't')",
            [r#"{"Version":"2026-10-25","Statement":[{"Effect":"Allow","Action":"moderation:act","Resource":"jurisdiction:42*"}]}"#],
        )
        .await
        .unwrap();
        attach_policy(&tx, &scoped.user_id, "ModeracaoSC").await;
        tx.commit().await.unwrap();
        let scoped = fresh(&t, &scoped).await;
        let conn = t.db.read().await.unwrap();
        let mut ids = Vec::with_capacity(2);
        for code in ["4202404", "BR"] {
            ids.push(
                query_opt(
                    &conn,
                    "SELECT id FROM jurisdictions WHERE code = ?1",
                    [code],
                )
                .await
                .unwrap()
                .unwrap()
                .get::<String>(0)
                .unwrap(),
            );
        }
        assert!(!scoped.can(Permission::ModerationAct), "no global grant");
        assert_eq!(may_act(&scoped, Permission::ModerationAct), Ok(()));
        assert_eq!(
            require_in_jurisdiction(&conn, &scoped, Permission::ModerationAct, &ids[0]).await,
            Ok(())
        );
        assert_eq!(
            require_in_jurisdiction(&conn, &scoped, Permission::ModerationAct, &ids[1]).await,
            Err(AppError::Forbidden)
        );
        let citizen = account(&t, "active", &["Cidadao"]).await;
        assert_eq!(
            may_act(&citizen, Permission::ModerationAct),
            Err(AppError::Forbidden)
        );
    }

    #[tokio::test]
    async fn cli_bootstrap_attaches_administrator_access_to_active_accounts() {
        let t = world().await;
        let citizen = account(&t, "active", &["Cidadao"]).await;
        let pending = account(&t, "pending", &[]).await;
        let citizen_email = email(&t, &citizen).await;
        let pending_email = email(&t, &pending).await;
        assert!(
            grant_administrator(&t.db, now(), "missing@example.test")
                .await
                .is_err()
        );
        assert!(
            grant_administrator(&t.db, now(), &pending_email)
                .await
                .is_err()
        );
        grant_administrator(&t.db, now(), &citizen_email)
            .await
            .unwrap();
        assert!(matches!(
            grant_administrator(&t.db, now(), &citizen_email).await,
            Err(AppError::Conflict(_))
        ));
        assert!(fresh(&t, &citizen).await.can(Permission::FlagsManage));
        assert_eq!(
            iam_events(&t).await,
            vec![(
                "iam.user_policy_attached".into(),
                "restricted".into(),
                "operator:cli".into()
            )]
        );
    }

    #[tokio::test]
    async fn electorate_counts_active_cidadao_holders_directly_or_by_group() {
        let t = world().await;
        account(&t, "active", &["Cidadao"]).await;
        account(&t, "active", &["AdministratorAccess"]).await;
        account(&t, "pending", &["Cidadao"]).await;
        let via_group = account(&t, "active", &["Moderador"]).await;
        let tx = t.db.write().await.unwrap();
        tx.execute_batch(
            "INSERT INTO iam_groups (id, name, created_at) VALUES ('g1', 'Cidadania', 't');
             INSERT INTO iam_group_policies (group_id, policy_id, attached_at) VALUES ('g1', 'pol-cidadao', 't');",
        )
        .await
        .unwrap();
        tx.execute(
            "INSERT INTO iam_user_groups (user_id, group_id, added_at) VALUES (?1, 'g1', 't')",
            [via_group.user_id.as_str()],
        )
        .await
        .unwrap();
        attach_policy(&tx, &via_group.user_id, "Auditor").await;
        tx.commit().await.unwrap();
        let conn = t.db.read().await.unwrap();
        let br: String = query_opt(&conn, "SELECT id FROM jurisdictions WHERE code = 'BR'", ())
            .await
            .unwrap()
            .unwrap()
            .get(0)
            .unwrap();
        assert_eq!(
            super::super::phases::electorate_count(&conn, &br)
                .await
                .unwrap(),
            2
        );
    }
}
