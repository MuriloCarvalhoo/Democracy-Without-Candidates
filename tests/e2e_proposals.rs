//! Proposal browser regressions (Fantoccini/WebDriver). Run with `just e2e`.
//! Signing/publishing through the UI depends on the identity feature's
//! `ReauthPanel` and is covered by Rust use-case tests until it lands.

mod support;

use fantoccini::Locator;
use support::{App, Browser, http_get, http_post, seed_dev};

#[tokio::test]
#[ignore = "requires built app and chromedriver; run with `just e2e`"]
async fn visitor_lists_filters_and_reads_seeded_proposals() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("e2e.db");
    seed_dev(&db);
    let app = App::start(&db, &[]);
    let browser = Browser::start().await;

    // Default listing hides archived proposals: 2 collecting, debate, voting, expired.
    browser.goto_hydrated(&app.url("/propostas")).await;
    browser
        .wait_text("[data-testid=proposal-total]", |t| t.starts_with("5 "))
        .await;

    // Filter after hydration: client-side navigation, no full reload.
    browser
        .client
        .execute("window.__noReload = true;", vec![])
        .await
        .unwrap();
    browser
        .client
        .find(Locator::Css("[data-testid=filter-status]"))
        .await
        .unwrap()
        .select_by_value("in_debate")
        .await
        .unwrap();
    browser
        .client
        .find(Locator::Css("[data-testid=filter-apply]"))
        .await
        .unwrap()
        .click()
        .await
        .unwrap();
    browser
        .wait_text("[data-testid=proposal-total]", |t| t.starts_with("1 "))
        .await;
    assert!(
        browser
            .text_of("[data-testid=proposal-item]")
            .await
            .contains("Reforma da praça central")
    );
    let same_document = browser
        .client
        .execute("return window.__noReload === true;", vec![])
        .await
        .unwrap();
    assert_eq!(same_document, serde_json::Value::Bool(true));
    let url = browser.client.current_url().await.unwrap();
    assert!(url.query().unwrap_or_default().contains("status=in_debate"));

    // Detail: text, status badge and signature progress.
    browser
        .client
        .find(Locator::Css("[data-testid=proposal-item] h3 a"))
        .await
        .unwrap()
        .click()
        .await
        .unwrap();
    browser
        .wait_text("[data-testid=proposal-title]", |t| t.contains("Reforma"))
        .await;
    assert!(
        browser
            .text_of("[data-testid=proposal-body]")
            .await
            .contains("Art. 1º")
    );
    assert_eq!(
        browser.text_of("[data-testid=proposal-status]").await,
        "Em debate"
    );
    assert!(
        browser
            .text_of("[data-testid=signature-count]")
            .await
            .starts_with("1 de 1")
    );
    let detail_url = browser.client.current_url().await.unwrap();
    let id = detail_url
        .path()
        .trim_start_matches("/propostas/")
        .to_owned();

    // Markdown export with the published text hash.
    let (status, body) = http_get(app.port, &format!("/propostas/{id}/exportar"));
    assert_eq!(status, 200);
    assert!(body.contains("Reforma da praça central") && body.contains("SHA-256"));
    assert_eq!(http_get(app.port, "/propostas/nao-existe/exportar").0, 404);

    // Detail rendered by SSR hydrates: a router link then navigates client-side.
    browser.goto_hydrated(detail_url.as_str()).await;
    browser
        .client
        .execute("window.__noReload = true;", vec![])
        .await
        .unwrap();
    browser
        .client
        .find(Locator::Css(&format!(
            "a[href='/propostas/{id}/assinantes']"
        )))
        .await
        .unwrap()
        .click()
        .await
        .unwrap();
    assert_eq!(
        browser
            .wait_text("[data-testid=signer-count]", |t| !t.is_empty())
            .await,
        "1"
    );
    let same_document = browser
        .client
        .execute("return window.__noReload === true;", vec![])
        .await
        .unwrap();
    assert_eq!(same_document, serde_json::Value::Bool(true));

    // Level + status filters from the URL (SSR), and an empty result state.
    browser
        .goto_hydrated(&app.url("/propostas?level=national&status=expired"))
        .await;
    browser
        .wait_text("[data-testid=proposal-total]", |t| t.starts_with("1 "))
        .await;
    browser
        .goto_hydrated(&app.url("/propostas?kind=setup_change"))
        .await;
    browser
        .wait_text("[data-testid=proposal-total]", |t| t.starts_with("0 "))
        .await;

    // Public setup history shows the synthetic adopted version.
    browser.goto_hydrated(&app.url("/setup/historico")).await;
    assert!(
        browser
            .wait_text("[data-testid=setup-version]", |t| t.contains("Sintético"))
            .await
            .contains("Brasil")
    );

    // Visitor dashboard lists active phases.
    browser.goto_hydrated(&app.url("/dashboard")).await;
    browser
        .wait_text("main", |t| t.contains("Plano emergencial contra enchentes"))
        .await;
    browser.close().await;
}

/// Mutating server functions are public endpoints: they reject cross-origin
/// requests and callers without a valid session, whatever the client sends.
#[tokio::test]
#[ignore = "requires built app; run with `just e2e`"]
async fn mutating_endpoints_require_session_and_same_origin() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("e2e.db");
    seed_dev(&db);
    let app = App::start(&db, &[]);
    let origin = app.url("");
    let form = [
        ("Origin", origin.as_str()),
        ("Content-Type", "application/x-www-form-urlencoded"),
    ];
    let cross = http_post(
        app.port,
        "/api/proposals/sign",
        &[("Origin", "https://evil.example"), form[1]],
        "id=x",
    );
    assert_eq!(cross.0, 403);
    for headers in [
        form.to_vec(),
        [form.as_slice(), &[("Cookie", "dwc_session=forged")]].concat(),
    ] {
        for path in ["/api/proposals/sign", "/api/proposals/publish"] {
            let (status, body) = http_post(
                app.port,
                path,
                &headers,
                "id=00000000-0000-0000-0000-000000000000",
            );
            assert_ne!(status, 200, "{path}");
            assert!(body.contains("Unauthenticated"), "{path}: {body}");
        }
    }
    let json = [
        ("Origin", origin.as_str()),
        ("Content-Type", "application/json"),
    ];
    let draft = r#"{"id":null,"input":{"kind":"NormalLaw","level":"Municipal","title":"Título válido","summary":"Resumo válido aqui","body":"Corpo válido com mais de vinte caracteres","proposed_setup":null}}"#;
    let (status, body) = http_post(app.port, "/api/proposals/save_draft", &json, draft);
    assert_ne!(status, 200);
    assert!(body.contains("Unauthenticated"), "{body}");
}
