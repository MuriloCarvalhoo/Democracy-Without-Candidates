//! Responsive layout regressions: mobile (390px) and desktop (1280px) after hydration.
//! Run with `just e2e`.

mod support;

use fantoccini::Locator;
use serde_json::Value;
use support::{App, Browser, seed_dev};

const PAGES: &[&str] = &[
    "/",
    "/propostas",
    "/dashboard",
    "/auditoria",
    "/login",
    "/cadastro",
];

async fn js(b: &Browser, script: &str) -> Value {
    b.client.execute(script, vec![]).await.expect(script)
}

/// Pixels the document is wider than the viewport (0 = no horizontal scroll).
async fn overflow(b: &Browser) -> i64 {
    js(
        b,
        "return document.documentElement.scrollWidth - document.documentElement.clientWidth;",
    )
    .await
    .as_i64()
    .unwrap()
}

#[tokio::test]
#[ignore = "requires built app and chromedriver; run with `just e2e`"]
async fn layout_fits_mobile_and_desktop_with_reachable_navigation() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("e2e.db");
    seed_dev(&db);
    let app = App::start(&db, &[]);
    let b = Browser::start().await;

    // Mobile (Browser starts at 390x844): no horizontal page scroll on any page.
    for path in PAGES {
        b.goto_hydrated(&app.url(path)).await;
        assert_eq!(
            overflow(&b).await,
            0,
            "horizontal overflow at 390px on {path}"
        );
    }
    // A proposal detail page too.
    b.goto_hydrated(&app.url("/propostas")).await;
    let href = b
        .client
        .find(Locator::Css("[data-testid=proposal-item] h3 a"))
        .await
        .unwrap()
        .attr("href")
        .await
        .unwrap()
        .unwrap();
    b.goto_hydrated(&app.url(&href)).await;
    assert_eq!(
        overflow(&b).await,
        0,
        "horizontal overflow at 390px on {href}"
    );

    // Home explains the proposal lifecycle in order and the footer is present.
    b.goto_hydrated(&app.url("/")).await;
    let steps = js(
        &b,
        "return document.querySelectorAll('[data-testid=lifecycle] > li').length;",
    )
    .await;
    assert_eq!(steps, Value::from(5));
    assert!(
        !b.text_of("footer.site-footer").await.is_empty(),
        "footer visible"
    );

    // Account actions stay visible on mobile; every primary link is reachable
    // (scrolled into view and clickable) without a menu.
    assert_eq!(b.text_of("[data-testid=nav-login]").await, "Entrar");
    let audit = b
        .client
        .find(Locator::Css(".primary-nav a[href='/auditoria']"))
        .await
        .unwrap();
    audit.click().await.unwrap();
    let start = std::time::Instant::now();
    while js(&b, "return location.pathname;").await != "/auditoria" {
        assert!(start.elapsed() < support::WAIT, "navigation to /auditoria");
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    let current = js(
        &b,
        "return document.querySelector('.primary-nav a[aria-current=page]')?.getAttribute('href');",
    )
    .await;
    assert_eq!(current, Value::from("/auditoria"));

    // Tap targets: header links and buttons are at least 44px tall.
    let small = js(
        &b,
        "return [...document.querySelectorAll('.site-header a, .site-header button')]
            .filter(e => e.offsetParent && e.getBoundingClientRect().height < 44)
            .map(e => e.textContent.trim());",
    )
    .await;
    assert_eq!(small, Value::Array(vec![]), "header tap targets under 44px");

    // Desktop: header in one row; content limited to a readable max width.
    b.client.set_window_size(1280, 900).await.unwrap();
    b.goto_hydrated(&app.url("/propostas")).await;
    assert_eq!(overflow(&b).await, 0, "horizontal overflow at 1280px");
    let one_row = js(
        &b,
        "const t = s => document.querySelector(s).getBoundingClientRect();
         const [a, n, c] = [t('.site-header .brand'), t('.primary-nav'), t('.account-nav')];
         return Math.abs(a.top - n.top) < 24 && Math.abs(n.top - c.top) < 24;",
    )
    .await;
    assert_eq!(one_row, Value::Bool(true), "desktop header on one row");
    let main_width = js(
        &b,
        "return document.querySelector('main').getBoundingClientRect().width;",
    )
    .await
    .as_f64()
    .unwrap();
    assert!(main_width <= 1200.0, "main too wide: {main_width}");

    b.close().await;
}
