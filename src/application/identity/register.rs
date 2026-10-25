//! Registration: e-mail, password, name and CEP. The address persisted is the
//! server-side provider result for the submitted CEP; territory is derived from it
//! (IBGE code, else exact city + UF in the local catalog). Country comes from the
//! instance configuration.

use turso::Connection;

use super::Deps;
use crate::{
    application::{
        audit::{self, Event, Visibility},
        session,
    },
    domain::identity::{normalize_email, normalize_name, password_acceptable},
    error::AppError,
    i18n::t,
    infrastructure::{
        address::{AddressProvider, CepAddress},
        clock::ts,
        crypto::new_id,
        db::{is_constraint, query_all},
        password,
    },
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Territory {
    pub municipality_id: String,
    pub state_id: String,
    pub country_id: String,
    pub municipality_name: String,
}

/// Resolves the municipality without fuzzy matching. Absence, ambiguity or a UF
/// inconsistent with the IBGE code blocks registration.
#[tracing::instrument(level = "debug", skip_all, fields(country_code = %country_code), err(level = "debug"))]
pub async fn resolve_territory(
    conn: &Connection,
    address: &CepAddress,
    country_code: &str,
) -> Result<Territory, AppError> {
    const SELECT: &str = "SELECT m.id, s.id, n.id, m.name, s.state_abbrev FROM jurisdictions m
        JOIN jurisdictions s ON s.id = m.parent_id AND s.kind = 'state'
        JOIN jurisdictions n ON n.id = s.parent_id AND n.kind = 'national'
        WHERE m.kind = 'municipal' AND m.active = 1 AND n.code = ?1";
    let rows = match address.ibge_city() {
        Some(code) => {
            query_all(
                conn,
                &format!("{SELECT} AND m.code = ?2"),
                (country_code, code.as_str()),
            )
            .await?
        }
        None => {
            query_all(
                conn,
                &format!("{SELECT} AND m.name = ?2 AND s.state_abbrev = ?3"),
                (country_code, address.city.as_str(), address.state.as_str()),
            )
            .await?
        }
    };
    let [row] = rows.as_slice() else {
        return Err(AppError::validation("cep", t("auth.cep.unresolved")));
    };
    if row.get::<Option<String>>(4)?.as_deref() != Some(address.state.as_str()) {
        return Err(AppError::validation("cep", t("auth.cep.unresolved")));
    }
    Ok(Territory {
        municipality_id: row.get(0)?,
        state_id: row.get(1)?,
        country_id: row.get(2)?,
        municipality_name: row.get(3)?,
    })
}

/// Address lookup + territory check, used by the CEP search and by registration.
#[tracing::instrument(level = "debug", skip_all, fields(country_code = %country_code), err(level = "debug"))]
pub async fn lookup(
    deps: &Deps<'_>,
    provider: Option<&AddressProvider>,
    country_code: &str,
    cep: &str,
) -> Result<(CepAddress, Territory), AppError> {
    let provider = provider.ok_or_else(|| AppError::Unavailable(t("auth.cep.disabled").into()))?;
    let address = provider.lookup(cep).await?;
    let conn = deps.db.read().await?;
    let territory = resolve_territory(&conn, &address, country_code).await?;
    Ok((address, territory))
}

pub struct Registration<'a> {
    pub name: &'a str,
    pub email: &'a str,
    pub password: &'a str,
    pub cep: &'a str,
}

pub struct Registered {
    pub user_id: String,
    pub session_token: String,
}

/// Creates a pending account (CITIZEN role, no participation until enrollment)
/// and a session to continue with face + TOTP enrollment.
#[tracing::instrument(level = "debug", skip_all, fields(country_code = %country_code), err(level = "debug"))]
pub async fn register(
    deps: &Deps<'_>,
    provider: Option<&AddressProvider>,
    country_code: &str,
    input: Registration<'_>,
) -> Result<Registered, AppError> {
    let name = normalize_name(input.name)
        .ok_or_else(|| AppError::validation("name", t("auth.name.invalid")))?;
    let email = normalize_email(input.email)
        .ok_or_else(|| AppError::validation("email", t("auth.email.invalid")))?;
    if !password_acceptable(input.password) {
        return Err(AppError::validation("password", t("auth.password.policy")));
    }
    let (address, territory) = lookup(deps, provider, country_code, input.cep).await?;
    let password_hash = password::hash(input.password).await?;

    let user_id = new_id();
    let now = ts(deps.now);
    let tx = deps.db.write().await?;
    let inserted = tx
        .execute(
            "INSERT INTO users (id, email, name, password_hash, status, municipality_id, state_id, country_id, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, 'pending', ?5, ?6, ?7, ?8, ?8)",
            (
                user_id.as_str(),
                email.as_str(),
                name.as_str(),
                password_hash,
                territory.municipality_id.as_str(),
                territory.state_id.as_str(),
                territory.country_id.as_str(),
                now.as_str(),
            ),
        )
        .await;
    match inserted {
        Ok(_) => {}
        Err(e) if is_constraint(&e) => {
            // Same message whether or not the e-mail belongs to someone else.
            return Err(AppError::Conflict(t("auth.register.unavailable").into()));
        }
        Err(e) => return Err(e.into()),
    }
    tx.execute(
        "INSERT INTO user_roles (user_id, role, granted_at) VALUES (?1, 'CITIZEN', ?2)",
        (user_id.as_str(), now.as_str()),
    )
    .await?;
    tx.execute(
        "INSERT INTO addresses (user_id, cep, state, city, neighborhood, street, service, ibge, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        (
            user_id.as_str(),
            address.cep.as_str(),
            address.state.as_str(),
            address.city.as_str(),
            address.neighborhood.as_str(),
            address.street.as_str(),
            address.service.as_str(),
            address.ibge.to_string(),
            now.as_str(),
        ),
    )
    .await?;
    tx.execute(
        "INSERT INTO user_preferences (user_id, updated_at) VALUES (?1, ?2)",
        (user_id.as_str(), now.as_str()),
    )
    .await?;
    audit::append(
        &tx,
        deps.now,
        Event {
            kind: "identity.account_created",
            visibility: Visibility::Restricted,
            actor_ref: Some(&user_id),
            subject: Some(&user_id),
            payload: serde_json::json!({}),
        },
    )
    .await?;
    let session_token = session::create(&tx, deps.now, deps.policy, &user_id).await?;
    tx.commit().await?;
    Ok(Registered {
        user_id,
        session_token,
    })
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{super::testing::*, *};
    use crate::infrastructure::{address::testing::mock_brasilapi, db::query_opt};

    async fn provider() -> AddressProvider {
        AddressProvider::brasilapi(&mock_brasilapi().await, Duration::from_millis(500))
    }

    fn input<'a>(email: &'a str, cep: &'a str) -> Registration<'a> {
        Registration {
            name: "Ana Sintética",
            email,
            password: PASSWORD,
            cep,
        }
    }

    #[tokio::test]
    async fn registers_pending_citizen_with_server_side_address() {
        let w = World::new().await;
        let p = provider().await;
        let r = register(
            &w.deps(),
            Some(&p),
            "BR",
            input(" Ana@Example.org ", "89010-025"),
        )
        .await
        .unwrap();
        let conn = w.t.db.read().await.unwrap();
        let row = query_opt(
            &conn,
            "SELECT u.email, u.status, m.code, a.cep, a.city, a.service, a.ibge, r.role, p.default_vote_visibility
             FROM users u JOIN jurisdictions m ON m.id = u.municipality_id JOIN addresses a ON a.user_id = u.id
             JOIN user_roles r ON r.user_id = u.id JOIN user_preferences p ON p.user_id = u.id WHERE u.id = ?1",
            [r.user_id.as_str()],
        )
        .await
        .unwrap()
        .unwrap();
        let got: Vec<String> = (0..9).map(|i| row.get::<String>(i).unwrap()).collect();
        assert_eq!(
            got,
            [
                "ana@example.org",
                "pending",
                "4202404",
                "89010025",
                "Blumenau",
                "open-cep",
                r#"{"city":"4202404","state":"42"}"#,
                "CITIZEN",
                "private"
            ]
        );
        let actor = session::resolve(&w.t.db, w.now, w.deps().policy, &r.session_token, false)
            .await
            .unwrap()
            .unwrap();
        assert!(!actor.can(crate::domain::identity::Permission::ProposalVote));
    }

    #[tokio::test]
    async fn territory_fallback_generic_cep_and_blocking_cases() {
        let w = World::new().await;
        let p = provider().await;
        let conn = w.t.db.read().await.unwrap();
        // No IBGE code: exact city + UF.
        let (_, t1) = lookup(&w.deps(), Some(&p), "BR", "88010000").await.unwrap();
        assert_eq!(t1.municipality_name, "Florianópolis");
        // Generic CEP with empty street/neighborhood is accepted.
        let (a, t2) = lookup(&w.deps(), Some(&p), "BR", "89000000").await.unwrap();
        assert_eq!(
            (a.street.as_str(), t2.municipality_name.as_str()),
            ("", "Blumenau")
        );
        // Unknown city, UF inconsistent with IBGE code, other country catalog.
        for cep in ["33333333", "44444444"] {
            assert!(matches!(
                lookup(&w.deps(), Some(&p), "BR", cep).await,
                Err(AppError::Validation { .. })
            ));
        }
        let address = p.lookup("89010025").await.unwrap();
        assert!(resolve_territory(&conn, &address, "AR").await.is_err());
        // No fuzzy matching on names.
        let fuzzy = CepAddress {
            city: "blumenau".into(),
            ibge: serde_json::json!({"city": null, "state": null}),
            ..address
        };
        assert!(resolve_territory(&conn, &fuzzy, "BR").await.is_err());
        // Provider disabled.
        assert!(matches!(
            lookup(&w.deps(), None, "BR", "89010025").await,
            Err(AppError::Unavailable(_))
        ));
    }

    #[tokio::test]
    async fn invalid_inputs_and_duplicates_create_nothing() {
        let w = World::new().await;
        let p = provider().await;
        let bad = [
            Registration {
                name: "A",
                ..input("a@example.org", "89010025")
            },
            input("not-an-email", "89010025"),
            Registration {
                password: "short",
                ..input("a@example.org", "89010025")
            },
            input("a@example.org", "00000000"),
            input("a@example.org", "123"),
        ];
        for r in bad {
            assert!(register(&w.deps(), Some(&p), "BR", r).await.is_err());
        }
        register(
            &w.deps(),
            Some(&p),
            "BR",
            input("dup@example.org", "89010025"),
        )
        .await
        .unwrap();
        let dup = register(
            &w.deps(),
            Some(&p),
            "BR",
            input("DUP@example.org", "88010000"),
        )
        .await;
        assert_eq!(
            dup.err(),
            Some(AppError::Conflict(t("auth.register.unavailable").into()))
        );
        let conn = w.t.db.read().await.unwrap();
        let n: i64 = query_opt(&conn, "SELECT COUNT(*) FROM users", ())
            .await
            .unwrap()
            .unwrap()
            .get(0)
            .unwrap();
        let addresses: i64 = query_opt(&conn, "SELECT COUNT(*) FROM addresses", ())
            .await
            .unwrap()
            .unwrap()
            .get(0)
            .unwrap();
        assert_eq!((n, addresses), (1, 1));
    }
}
