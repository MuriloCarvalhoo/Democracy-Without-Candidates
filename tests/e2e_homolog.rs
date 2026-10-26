//! Homologation seed in the browser: `dwc seed-homolog` on a disposable database,
//! then level filters and a law-based proposal detail. Run with `just e2e`.
//! Screenshots go to `artifacts/homolog/` (gitignored).

mod support;

use std::{
    path::Path,
    process::{Command, Stdio},
};

use fantoccini::Locator;
use support::{App, Browser, free_port};

/// Same isolated environment as the E2E app (test env, synthetic identity).
fn seed_homolog(db_path: &Path) {
    let bin = std::env::var("DWC_E2E_BIN").unwrap_or_else(|_| "target/e2e/dwc".into());
    let status = Command::new(&bin)
        .env_clear()
        .env("PATH", std::env::var("PATH").unwrap_or_default())
        .env("RUST_LOG", "warn")
        .env("DWC_ENV", "test")
        .env("DWC_DB_PATH", db_path)
        .env("DWC_COUNTRY", "BR")
        .env("DWC_IDENTITY_PROVIDER", "synthetic")
        .env("LEPTOS_SITE_ROOT", "target/site")
        .env("LEPTOS_OUTPUT_NAME", "dwc")
        .env("LEPTOS_SITE_ADDR", format!("127.0.0.1:{}", free_port()))
        .arg("seed-homolog")
        .stdout(Stdio::null())
        .status()
        .unwrap_or_else(|e| panic!("run {bin} seed-homolog: {e}"));
    assert!(status.success(), "seed-homolog failed: {status}");
}

async fn screenshot(browser: &Browser, name: &str) {
    let png = browser.client.screenshot().await.expect("screenshot");
    std::fs::create_dir_all("artifacts/homolog").unwrap();
    std::fs::write(format!("artifacts/homolog/{name}.png"), png).unwrap();
}

async fn items(browser: &Browser) -> Vec<String> {
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
}

#[tokio::test]
#[ignore = "requires built app and chromedriver; run with `just e2e`"]
async fn homologation_laws_are_listed_by_level_and_cite_the_official_source() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("homolog.db");
    seed_homolog(&db);
    seed_homolog(&db); // idempotent: a second run adds nothing
    let app = App::start(&db, &[]);
    let browser = Browser::start().await;

    // State filter through the hydrated form: state-level homologation laws.
    browser.goto_hydrated(&app.url("/propostas")).await;
    browser
        .client
        .find(Locator::Css("[data-testid=filter-level]"))
        .await
        .unwrap()
        .select_by_value("state")
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
    let total = browser
        .wait_text("[data-testid=proposal-total]", |t| {
            t.split_whitespace()
                .next()
                .and_then(|n| n.parse::<u32>().ok())
                .is_some_and(|n| n >= 81)
        })
        .await;
    let state_items = items(&browser).await;
    assert!(
        state_items
            .iter()
            .any(|t| t.contains("Demonstração de homologação")),
        "state list shows homologation laws ({total})"
    );
    screenshot(&browser, "propostas-estaduais").await;

    // National level from the URL (SSR): national laws, with a closed vote result.
    browser
        .goto_hydrated(&app.url("/propostas?level=national&status=approved"))
        .await;
    browser
        .wait_text("[data-testid=proposal-item]", |t| {
            t.contains("Demonstração de homologação")
        })
        .await;
    screenshot(&browser, "propostas-nacionais-aprovadas").await;

    browser
        .goto_hydrated(&app.url("/propostas?level=national"))
        .await;
    browser
        .wait_text("[data-testid=proposal-item]", |t| {
            t.contains("Demonstração de homologação")
        })
        .await;
    screenshot(&browser, "propostas-nacionais").await;

    // Detail: official ementa, generated summary and the official source URL.
    let link = browser
        .client
        .find(Locator::XPath(
            "//li[@data-testid='proposal-item'][contains(., 'Demonstração de homologação')]//h3/a",
        ))
        .await
        .unwrap();
    link.click().await.unwrap();
    let body = browser
        .wait_text("[data-testid=proposal-body]", |t| {
            t.contains("Fonte oficial:")
        })
        .await;
    assert!(body.contains("não é uma iniciativa popular real"));
    assert!(body.contains("Fonte oficial: https://www.planalto.gov.br/"));
    assert!(body.contains("Ementa oficial:"));
    screenshot(&browser, "proposta-detalhe").await;
    browser.close().await;
}
