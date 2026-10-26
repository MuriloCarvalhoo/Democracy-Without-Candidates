//! Proposal browser regressions (Fantoccini/WebDriver). Run with `just e2e`.
//! Signing/publishing through the UI depends on the identity feature's
//! `ReauthPanel` and is covered by Rust use-case tests until it lands.

mod support;

use dwc::infrastructure::db::Db;
use fantoccini::Locator;
use support::{App, Browser, http_get, http_post, seed_dev, session_for, use_session};

#[tokio::test]
#[ignore = "requires built app and chromedriver; run with `just e2e`"]
async fn visitor_lists_filters_and_reads_seeded_proposals() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("e2e.db");
    seed_dev(&db);
    let app = App::start(&db, &[]);
    let browser = Browser::start().await;

    // Default listing hides archived proposals: 2 collecting, debate, 2 voting,
    // expired and one closed (approved) vote.
    browser.goto_hydrated(&app.url("/propostas")).await;
    browser
        .wait_text("[data-testid=proposal-total]", |t| t.starts_with("7 "))
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

/// The detail aside never shows an empty action card: a signed-in viewer outside
/// the electorate gets the eligibility notice and no actions container.
#[tokio::test]
#[ignore = "requires built app and chromedriver; run with `just e2e`"]
async fn visitor_filters_by_state_and_city() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("e2e.db");
    seed_dev(&db);
    let app = App::start(&db, &[]);
    let browser = Browser::start().await;
    let items = async || {
        let mut out = Vec::new();
        for el in browser
            .client
            .find_all(Locator::Css("[data-testid=proposal-item]"))
            .await
            .unwrap()
        {
            out.push(el.text().await.unwrap());
        }
        out
    };

    browser.goto_hydrated(&app.url("/propostas")).await;
    let select = async |css: &str, value: &str| {
        browser
            .client
            .find(Locator::Css(css))
            .await
            .unwrap()
            .select_by_value(value)
            .await
            .unwrap();
    };
    assert!(
        browser
            .text_of("[data-testid=filter-country]")
            .await
            .contains("Brasil")
    );
    // City options load after the state is picked (no navigation needed).
    select("[data-testid=filter-state]", "42").await;
    browser
        .wait_text("[data-testid=filter-city]", |t| t.contains("Blumenau"))
        .await;
    select("[data-testid=filter-city]", "4202404").await;
    browser
        .client
        .find(Locator::Css("[data-testid=filter-apply]"))
        .await
        .unwrap()
        .click()
        .await
        .unwrap();
    browser
        .wait_text("[data-testid=proposal-total]", |t| !t.starts_with("7 "))
        .await;
    let url = browser.client.current_url().await.unwrap();
    assert!(
        url.query()
            .is_some_and(|q| q.contains("state=42") && q.contains("city=4202404"))
    );
    let city = items().await;
    assert!(!city.is_empty() && city.iter().all(|t| t.contains("Blumenau")));

    // State from the URL (SSR): state laws and its cities, never national ones.
    browser.goto_hydrated(&app.url("/propostas?state=42")).await;
    let state = items().await;
    assert!(state.len() > city.len(), "{state:#?}");
    assert!(state.iter().all(|t| !t.contains("Nacional")), "{state:#?}");
    browser.goto_hydrated(&app.url("/propostas?state=35")).await;
    browser
        .wait_text("[data-testid=proposal-total]", |t| t.starts_with("0 "))
        .await;
    assert!(browser.console_errors().await.is_empty());
    browser.close().await;
}

#[tokio::test]
#[ignore = "requires built app and chromedriver; run with `just e2e`"]
async fn viewer_without_actions_sees_no_empty_action_card() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("e2e.db");
    seed_dev(&db);
    let (outsider, citizen) = {
        let db = Db::open(&db).await.unwrap();
        (
            session_for(&db, "master@dev.invalid").await,
            session_for(&db, "ana@dev.invalid").await,
        )
    };
    let app = App::start(&db, &[]);
    let browser = Browser::start().await;
    let empty_cards = "return [...document.querySelectorAll('.detail-side > *')]\
        .filter(e => e.textContent.trim() === '').length;";

    browser
        .goto_hydrated(&app.url("/propostas?status=collecting_signatures"))
        .await;
    browser
        .wait_text("[data-testid=proposal-total]", |t| !t.is_empty())
        .await;
    let href = browser
        .client
        .execute(
            "return [...document.querySelectorAll('[data-testid=proposal-item] h3 a')]\
                .find(a => a.textContent.includes('Ciclovia')).getAttribute('href');",
            vec![],
        )
        .await
        .unwrap();
    let detail = app.url(href.as_str().expect("ciclovia link"));

    for (token, eligible) in [(outsider, false), (citizen, true)] {
        use_session(&browser, &app, &token).await;
        browser.goto_hydrated(&detail).await;
        browser
            .wait_text("[data-testid=proposal-title]", |t| t.contains("Ciclovia"))
            .await;
        let head = browser.text_of(".proposal-head").await;
        assert_eq!(!head.contains("eleitorado"), eligible, "{head}");
        let empty = browser.client.execute(empty_cards, vec![]).await.unwrap();
        assert_eq!(empty, serde_json::json!(0), "eligible={eligible}");
        let actions = browser
            .client
            .find(Locator::Css(".detail-side .card.actions"))
            .await
            .is_ok();
        assert_eq!(actions, eligible, "action card shown only with actions");
    }
    // `use_session` opens /healthz (JSON), whose implicit favicon request 404s.
    let errors: Vec<String> = browser
        .console_errors()
        .await
        .into_iter()
        .filter(|e| !e.contains("/favicon.ico"))
        .collect();
    assert!(errors.is_empty(), "{errors:#?}");
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
