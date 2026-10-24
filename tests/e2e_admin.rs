//! Admin panel regressions in a real browser. Run with `just e2e`.
//! The admin session is created directly in the disposable database before the
//! app starts (identity/login is another feature) and injected as a cookie.

mod support;

use dwc::{
    application::session::{self, SessionPolicy},
    infrastructure::{
        catalog,
        clock::ts,
        db::{Db, query_opt},
        migrations,
    },
};
use fantoccini::{Locator, cookies::Cookie};
use support::{App, Browser};

/// Synthetic active COUNTRY_ADMIN with a session; returns the cookie token.
async fn seed_admin(path: &std::path::Path) -> String {
    let db = Db::open(path).await.unwrap();
    migrations::run(&db).await.unwrap();
    catalog::import(&db, "catalog/br/jurisdictions.csv")
        .await
        .unwrap();
    let now = chrono::Utc::now();
    let tx = db.write().await.unwrap();
    let row = query_opt(
        &tx,
        "SELECT m.id, s.id, s.parent_id FROM jurisdictions m JOIN jurisdictions s ON s.id = m.parent_id
         WHERE m.kind = 'municipal' ORDER BY m.code LIMIT 1",
        (),
    )
    .await
    .unwrap()
    .unwrap();
    let (m, s, c): (String, String, String) = (
        row.get(0).unwrap(),
        row.get(1).unwrap(),
        row.get(2).unwrap(),
    );
    tx.execute(
        "INSERT INTO users (id, email, name, password_hash, status, municipality_id, state_id, country_id, synthetic, created_at, updated_at)
         VALUES ('admin-e2e', 'admin@example.test', 'Admin Sintético', 'x', 'active', ?1, ?2, ?3, 1, ?4, ?4)",
        (m, s, c, ts(now)),
    )
    .await
    .unwrap();
    tx.execute(
        "INSERT INTO user_roles (user_id, role, granted_at) VALUES ('admin-e2e', 'COUNTRY_ADMIN', ?1)",
        [ts(now)],
    )
    .await
    .unwrap();
    let token = session::create(
        &tx,
        now,
        SessionPolicy::from_secs(900, 8 * 3600),
        "admin-e2e",
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    token
}

#[tokio::test]
#[ignore = "requires built app and chromedriver; run with `just e2e`"]
async fn visitor_is_denied_admin_panel_after_hydration() {
    let dir = tempfile::tempdir().unwrap();
    let app = App::start(&dir.path().join("e2e.db"), &[]);
    let browser = Browser::start().await;

    browser.goto_hydrated(&app.url("/admin")).await;
    let text = browser
        .wait_text("[data-testid=admin-denied]", |t| !t.is_empty())
        .await;
    assert!(text.contains("Entre na sua conta"), "{text}");
    assert!(
        browser
            .client
            .find(Locator::Css("[data-testid=flag-proposals]"))
            .await
            .is_err()
    );
    browser.goto_hydrated(&app.url("/admin/auditores")).await;
    browser
        .wait_text("[data-testid=admin-denied]", |t| !t.is_empty())
        .await;
    browser.close().await;
}

#[tokio::test]
#[ignore = "requires built app and chromedriver; run with `just e2e`"]
async fn admin_toggle_is_pending_until_restart() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("e2e.db");
    let token = seed_admin(&db_path).await;
    let mut app = App::start(&db_path, &[]);
    let browser = Browser::start().await;

    browser.goto_hydrated(&app.url("/")).await;
    // Explicit domain and SameSite: fantoccini otherwise sends values Chrome rejects
    // (null domain, SameSite=None without Secure).
    let cookie = Cookie::parse(format!(
        "dwc_session={token}; Domain=127.0.0.1; Path=/; HttpOnly; SameSite=Lax"
    ))
    .unwrap();
    browser.client.add_cookie(cookie).await.unwrap();

    browser.goto_hydrated(&app.url("/admin")).await;
    let row = "[data-testid=flag-proposals]";
    assert_eq!(
        browser
            .wait_text(&format!("{row} [data-testid=flag-effective]"), |t| !t
                .is_empty())
            .await,
        "Desligada"
    );
    assert!(
        browser
            .client
            .find(Locator::Css("[data-testid=restart-pending]"))
            .await
            .is_err()
    );
    // Private voting is shown as not implemented and offers no toggle.
    let private = "[data-testid='flag-voting.private']";
    browser
        .wait_text(
            &format!("{private} [data-testid=flag-not-implemented]"),
            |t| t == "Não implementado",
        )
        .await;
    assert!(
        browser
            .client
            .find(Locator::Css(&format!(
                "{private} [data-testid=flag-toggle]"
            )))
            .await
            .is_err()
    );

    browser
        .client
        .execute("window.__noReload = true;", vec![])
        .await
        .unwrap();
    browser
        .client
        .find(Locator::Css(&format!("{row} [data-testid=flag-toggle]")))
        .await
        .unwrap()
        .click()
        .await
        .unwrap();
    browser
        .wait_text(&format!("{row} [data-testid=flag-stored]"), |t| {
            t == "Ligada"
        })
        .await;
    browser
        .wait_text(&format!("{row} [data-testid=flag-pending]"), |t| {
            t == "Pendente de reinício"
        })
        .await;
    assert_eq!(
        browser
            .text_of(&format!("{row} [data-testid=flag-effective]"))
            .await,
        "Desligada",
        "running process keeps the startup state"
    );
    browser
        .wait_text("[data-testid=restart-pending]", |t| !t.is_empty())
        .await;
    assert_eq!(
        browser
            .client
            .execute("return window.__noReload === true;", vec![])
            .await
            .unwrap(),
        serde_json::Value::Bool(true),
        "handled by the hydrated app"
    );

    // Operator restart outside the panel: the stored value becomes effective.
    app.restart();
    browser.goto_hydrated(&app.url("/admin")).await;
    browser
        .wait_text(&format!("{row} [data-testid=flag-effective]"), |t| {
            t == "Ligada"
        })
        .await;
    assert!(
        browser
            .client
            .find(Locator::Css(&format!("{row} [data-testid=flag-pending]")))
            .await
            .is_err()
    );
    browser.close().await;
}
