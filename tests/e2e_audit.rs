//! Browser regressions for transparency pages. Run with `just e2e`.

mod support;

use std::path::Path;

use dwc::{
    application::audit::{Event, Visibility, append},
    infrastructure::{db::Db, migrations},
};
use serde_json::json;
use support::{App, Browser, account_with_session, use_session};

/// Writes synthetic events with the real engine before the app owns the database.
async fn seed(path: &Path) {
    let db = Db::open(path).await.unwrap();
    migrations::run(&db).await.unwrap();
    let now = chrono::Utc::now();
    let tx = db.write().await.unwrap();
    let events = [
        ("proposal.created", Visibility::Public, "proposal:p1"),
        (
            "account.created",
            Visibility::Restricted,
            "restricted-subject",
        ),
        ("setup.adopted", Visibility::Public, "setup:v1"),
    ];
    for (kind, visibility, subject) in events {
        append(
            &tx,
            now,
            Event {
                kind,
                visibility,
                actor_ref: Some("actor-ref-hidden"),
                subject: Some(subject),
                payload: json!({ "synthetic": true }),
            },
        )
        .await
        .unwrap();
    }
    tx.commit().await.unwrap();
}

#[tokio::test]
#[ignore = "requires built app and chromedriver; run with `just e2e`"]
async fn visitor_sees_public_log_and_is_denied_restricted_view() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("e2e.db");
    seed(&db_path).await;
    let app = App::start(&db_path, &[]);
    let browser = Browser::start().await;

    browser.goto_hydrated(&app.url("/auditoria")).await;
    browser
        .wait_text("[data-testid=audit-chain-status]", |t| {
            t.contains("sem quebras")
        })
        .await;
    assert_eq!(
        browser.text_of("[data-testid=audit-chain-events]").await,
        "3"
    );
    assert_eq!(
        browser
            .text_of("[data-testid=audit-public-key]")
            .await
            .len(),
        64,
        "dev key generated next to the test database"
    );
    let table = browser.text_of("[data-testid=audit-events]").await;
    assert!(table.contains("proposal.created") && table.contains("setup.adopted"));
    assert!(
        !table.contains("restricted-subject"),
        "restricted event hidden"
    );
    assert!(!table.contains("actor-ref-hidden"), "actor refs hidden");

    browser.goto_hydrated(&app.url("/auditoria/avancada")).await;
    browser
        .wait_text("[data-testid=audit-denied]", |t| {
            t.contains("Entre na sua conta")
        })
        .await;
    assert!(
        browser
            .client
            .find(fantoccini::Locator::Css("[data-testid=audit-events]"))
            .await
            .is_err()
    );

    // A denied request does not append an access event.
    browser.goto_hydrated(&app.url("/auditoria")).await;
    assert_eq!(
        browser
            .wait_text("[data-testid=audit-chain-events]", |t| !t.is_empty())
            .await,
        "3"
    );
    browser.close().await;
}

async fn click(browser: &Browser, css: &str) {
    browser
        .client
        .wait()
        .at_most(support::WAIT)
        .for_element(fantoccini::Locator::Css(css))
        .await
        .expect(css)
        .click()
        .await
        .unwrap();
}

#[tokio::test]
#[ignore = "requires built app and chromedriver; run with `just e2e`"]
async fn citizen_notifications_and_auditor_restricted_access() {
    use dwc::{
        application::notify::{Notice, notify},
        infrastructure::catalog,
    };
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("e2e.db");
    seed(&db_path).await;
    let (citizen_token, auditor_id, auditor_token) = {
        let db = Db::open(&db_path).await.unwrap();
        catalog::import(&db, "catalog/br/jurisdictions.csv")
            .await
            .unwrap();
        let (citizen, citizen_token) =
            account_with_session(&db, "cidada@example.test", "pol-cidadao").await;
        let (auditor, auditor_token) =
            account_with_session(&db, "auditor@example.test", "pol-auditor").await;
        let tx = db.write().await.unwrap();
        for (event, title) in [
            ("proposal_created", "Nova proposta"),
            ("voting_opened", "Votação aberta"),
        ] {
            notify(
                &tx,
                chrono::Utc::now(),
                &citizen,
                Notice {
                    event,
                    title,
                    body: "Sintético",
                    link: None,
                },
            )
            .await
            .unwrap();
        }
        tx.commit().await.unwrap();
        (citizen_token, auditor, auditor_token)
    };
    let app = App::start(&db_path, &[]);
    let browser = Browser::start().await;

    // Citizen: badge, mark one as read (hydrated action), badge refresh.
    use_session(&browser, &app, &citizen_token).await;
    browser.goto_hydrated(&app.url("/notificacoes")).await;
    browser
        .wait_text("[data-testid=notifications-unread]", |t| t.starts_with('2'))
        .await;
    browser
        .wait_text("[data-testid=nav-unread]", |t| t.starts_with('2'))
        .await;
    // Marker proves the click is handled by the hydrated page (no hydration
    // error from invalid markup), not by a full-page form post.
    browser
        .client
        .execute("window.__noReload = true;", vec![])
        .await
        .unwrap();
    click(&browser, "[data-read=false] button[type=submit]").await;
    browser
        .wait_text("[data-testid=notifications-unread]", |t| t.starts_with('1'))
        .await;
    assert_eq!(
        browser
            .client
            .execute("return window.__noReload === true;", vec![])
            .await
            .unwrap(),
        serde_json::Value::Bool(true),
        "mark-as-read must run in the hydrated page"
    );
    browser
        .wait_text("[data-testid=nav-unread]", |t| t.starts_with('1'))
        .await;

    // Citizen: disabling a category persists.
    browser
        .goto_hydrated(&app.url("/perfil/notificacoes"))
        .await;
    click(&browser, "[data-testid=pref-proposal_created]").await;
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    browser
        .goto_hydrated(&app.url("/perfil/notificacoes"))
        .await;
    let checked = browser
        .client
        .wait()
        .at_most(support::WAIT)
        .for_element(fantoccini::Locator::Css("[data-testid=pref-voting_opened]"))
        .await
        .unwrap();
    assert_eq!(
        checked.prop("checked").await.unwrap().as_deref(),
        Some("true")
    );
    let unchecked = browser
        .client
        .find(fantoccini::Locator::Css(
            "[data-testid=pref-proposal_created]",
        ))
        .await
        .unwrap();
    assert_eq!(
        unchecked.prop("checked").await.unwrap().as_deref(),
        Some("false")
    );

    // Citizen cannot read restricted events.
    browser.goto_hydrated(&app.url("/auditoria/avancada")).await;
    browser
        .wait_text("[data-testid=audit-denied]", |t| t.contains("permissão"))
        .await;

    // Auditor reads restricted events; the access itself is logged first.
    use_session(&browser, &app, &auditor_token).await;
    browser.goto_hydrated(&app.url("/auditoria/avancada")).await;
    let table = browser
        .wait_text("[data-testid=audit-events]", |t| {
            t.contains("account.created")
        })
        .await;
    assert!(table.contains("audit.restricted_access") && table.contains(&auditor_id));
    browser.goto_hydrated(&app.url("/auditoria")).await;
    assert_eq!(
        browser
            .wait_text("[data-testid=audit-chain-events]", |t| !t.is_empty())
            .await,
        "4",
        "3 seeded + 1 auditor access"
    );
    browser.close().await;
}

/// Runs `dwc <args>` against `db_path` (service stopped) and returns (success, output).
fn dwc_cli(db_path: &Path, args: &[&str]) -> (bool, String) {
    let bin = std::env::var("DWC_E2E_BIN").unwrap_or_else(|_| "target/e2e/dwc".into());
    let out = std::process::Command::new(bin)
        .args(args)
        .env_clear()
        .env("PATH", std::env::var("PATH").unwrap_or_default())
        .env("RUST_LOG", "error")
        .env("DWC_ENV", "test")
        .env("DWC_DB_PATH", db_path)
        .env("DWC_COUNTRY", "BR")
        .env("DWC_IDENTITY_PROVIDER", "synthetic")
        .env("LEPTOS_SITE_ROOT", "target/site")
        .env("LEPTOS_OUTPUT_NAME", "dwc")
        .env("LEPTOS_SITE_ADDR", "127.0.0.1:0")
        .output()
        .expect("run dwc");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.success(), text)
}

#[tokio::test]
#[ignore = "requires built app; run with `just e2e`"]
async fn checkpoint_cli_exports_and_detects_rewritten_chain() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("instance.db");
    seed(&db_path).await;
    let export = dir.path().join("checkpoint.json");
    let (ok, out) = dwc_cli(&db_path, &["audit-checkpoint", export.to_str().unwrap()]);
    assert!(ok, "{out}");
    let cp: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&export).unwrap()).unwrap();
    assert_eq!(cp["seq"], 3);
    assert_eq!(cp["country"], "BR");
    let (ok, out) = dwc_cli(&db_path, &["audit-checkpoint", export.to_str().unwrap()]);
    assert!(
        !ok && out.contains("already exists"),
        "never overwrites: {out}"
    );

    let (ok, out) = dwc_cli(&db_path, &["audit-verify", export.to_str().unwrap()]);
    assert!(ok && out.contains("OK"), "{out}");

    // A different (internally valid) history no longer contains the exported head.
    let other = tempfile::tempdir().unwrap();
    let rewritten = other.path().join("instance.db");
    {
        let db = Db::open(&rewritten).await.unwrap();
        migrations::run(&db).await.unwrap();
        let tx = db.write().await.unwrap();
        for i in 0..3 {
            append(
                &tx,
                chrono::Utc::now(),
                Event {
                    kind: "proposal.created",
                    visibility: Visibility::Public,
                    actor_ref: None,
                    subject: Some("rewritten"),
                    payload: json!({ "i": i }),
                },
            )
            .await
            .unwrap();
        }
        tx.commit().await.unwrap();
    }
    let (ok, out) = dwc_cli(&rewritten, &["audit-verify", export.to_str().unwrap()]);
    assert!(!ok && out.contains("HeadMismatch"), "{out}");

    // Edited checkpoint fields break the signature.
    let forged = dir.path().join("forged.json");
    std::fs::write(
        &forged,
        std::fs::read_to_string(&export)
            .unwrap()
            .replace("\"seq\": 3", "\"seq\": 2"),
    )
    .unwrap();
    let (ok, out) = dwc_cli(&db_path, &["audit-verify", forged.to_str().unwrap()]);
    assert!(!ok && out.contains("BadSignature"), "{out}");
}
