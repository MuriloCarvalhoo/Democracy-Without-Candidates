//! PWA service worker regression (online-only). Run with `just e2e`.

mod support;

use std::time::{Duration, Instant};

use support::{App, Browser};

#[tokio::test]
#[ignore = "requires built app and chromedriver; run with `just e2e`"]
async fn service_worker_caches_only_hashed_assets_and_shows_offline_notice() {
    let dir = tempfile::tempdir().unwrap();
    let app = App::start(&dir.path().join("e2e.db"), &[]);
    let browser = Browser::start().await;

    // Not registered by default outside production.
    browser.goto_hydrated(&app.url("/")).await;
    let registered = browser
        .client
        .execute_async(
            "navigator.serviceWorker.getRegistrations().then(r => arguments[0](r.length))",
            vec![],
        )
        .await
        .unwrap();
    assert_eq!(registered, serde_json::json!(0));

    browser
        .client
        .execute("localStorage.setItem('dwc.sw', '1');", vec![])
        .await
        .unwrap();
    browser.goto_hydrated(&app.url("/")).await;
    let start = Instant::now();
    loop {
        let controlled = browser
            .client
            .execute("return !!navigator.serviceWorker.controller;", vec![])
            .await
            .unwrap();
        if controlled == serde_json::Value::Bool(true) {
            break;
        }
        assert!(
            start.elapsed() < support::WAIT,
            "service worker did not take control"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    // Page and server function traffic now passes through the worker.
    browser.goto_hydrated(&app.url("/admin")).await;
    browser
        .wait_text("[data-testid=admin-denied]", |t| !t.is_empty())
        .await;

    let cached = browser
        .client
        .execute_async(
            "const done = arguments[0];
             caches.keys().then(async keys => {
               const out = [];
               for (const k of keys) {
                 for (const r of await (await caches.open(k)).keys()) out.push(new URL(r.url).pathname);
               }
               done(out);
             });",
            vec![],
        )
        .await
        .unwrap();
    let cached: Vec<String> = serde_json::from_value(cached).unwrap();
    assert!(cached.contains(&"/offline.html".to_string()), "{cached:?}");
    assert!(
        cached
            .iter()
            .any(|p| p.starts_with("/pkg/dwc.") && p.ends_with(".wasm")),
        "{cached:?}"
    );
    assert!(
        cached.iter().all(|p| p == "/offline.html"
            || p == "/icon.svg"
            || p == "/manifest.webmanifest"
            || p.starts_with("/pkg/")),
        "pages or server function responses were cached: {cached:?}"
    );

    // Server gone: navigation shows the static offline notice, nothing is queued.
    let url = app.url("/admin");
    drop(app);
    browser.client.goto(&url).await.unwrap();
    assert_eq!(
        browser.wait_text("h1", |t| !t.is_empty()).await,
        "Sem conexão"
    );
    let body = browser.text_of("main").await;
    assert!(body.contains("Nenhuma ação foi guardada"), "{body}");
    browser.close().await;
}
