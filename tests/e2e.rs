//! Browser regressions (Fantoccini/WebDriver). Run with `just e2e`.

mod support;

use fantoccini::Locator;
use support::{App, Browser};

#[tokio::test]
#[ignore = "requires built app and chromedriver; run with `just e2e`"]
async fn foundation_hydrated_counter_persists_across_restart() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = App::start(&dir.path().join("e2e.db"), &[]);
    let browser = Browser::start().await;

    browser.goto_hydrated(&app.url("/")).await;
    assert_eq!(
        browser
            .wait_text("[data-testid=counter-value]", |t| !t.is_empty())
            .await,
        "0"
    );

    // Marker proves the click is handled by the hydrated app, not a full-page form post.
    browser
        .client
        .execute("window.__noReload = true;", vec![])
        .await
        .unwrap();
    browser
        .client
        .find(Locator::Css("[data-testid=counter-increment]"))
        .await
        .unwrap()
        .click()
        .await
        .unwrap();
    assert_eq!(
        browser
            .wait_text("[data-testid=counter-value]", |t| t == "1")
            .await,
        "1"
    );
    let still_same_document = browser
        .client
        .execute("return window.__noReload === true;", vec![])
        .await
        .unwrap();
    assert_eq!(still_same_document, serde_json::Value::Bool(true));

    app.restart();
    browser.goto_hydrated(&app.url("/")).await;
    assert_eq!(
        browser
            .wait_text("[data-testid=counter-value]", |t| !t.is_empty())
            .await,
        "1"
    );
    browser.close().await;
}
