//! Resilience regressions: every route hydrates without a browser panic for
//! visitor, citizen and admin (reload and client-side navigation), and a panic
//! leaves a visible notice, reaches the server log and navigation recovers.
//! Run with `just e2e`.

mod support;

use std::{path::Path, time::Duration};

use dwc::{
    application::session::{self, SessionPolicy},
    infrastructure::db::{Db, query_all, query_opt},
};
use fantoccini::{Locator, cookies::Cookie};
use support::{App, Browser, STDERR_FILE, WAIT, seed_dev};

const ROUTES: &[&str] = &[
    "/",
    "/auditoria",
    "/auditoria/avancada",
    "/notificacoes",
    "/perfil/notificacoes",
    "/login",
    "/recuperar-senha",
    "/cadastro",
    "/cadastro/biometria",
    "/perfil",
    "/perfil/privacidade",
    "/perfil/codigos",
    "/alterar-senha",
    "/propostas",
    "/propostas/nova",
    "/propostas/{id}",
    "/propostas/{id}/editar",
    "/propostas/{id}/assinantes",
    "/propostas/{id}/debate",
    "/propostas/{id}/debate/emenda",
    "/propostas/{id}/votar",
    "/propostas/{id}/resultado",
    "/metaleis",
    "/metaleis/nova",
    "/metaleis/{id}",
    "/setup",
    "/estatisticas",
    "/dashboard",
    "/setup/historico",
    "/admin",
    "/admin/auditores",
    "/admin/moderacao",
];

/// Sessions for seeded accounts (created before the app owns the database) and
/// the seeded proposal ids.
async fn sessions(path: &Path, emails: &[&str]) -> (Vec<String>, Vec<String>) {
    let db = Db::open(path).await.unwrap();
    let mut tokens = Vec::with_capacity(emails.len());
    let tx = db.write().await.unwrap();
    for email in emails {
        let id: String = query_opt(&tx, "SELECT id FROM users WHERE email = ?1", [*email])
            .await
            .unwrap()
            .unwrap()
            .get(0)
            .unwrap();
        tokens.push(
            session::create(
                &tx,
                chrono::Utc::now(),
                SessionPolicy::from_secs(3600, 8 * 3600),
                &id,
            )
            .await
            .unwrap(),
        );
    }
    tx.commit().await.unwrap();
    let ids = query_all(
        &db.read().await.unwrap(),
        "SELECT id FROM proposals ORDER BY created_at",
        (),
    )
    .await
    .unwrap()
    .iter()
    .map(|r| r.get::<String>(0).unwrap())
    .collect();
    (tokens, ids)
}

async fn set_session(browser: &Browser, app: &App, token: Option<&String>) {
    browser.client.goto(&app.url("/healthz")).await.unwrap();
    browser.client.delete_all_cookies().await.unwrap();
    if let Some(token) = token {
        let cookie = Cookie::parse(format!(
            "dwc_session={token}; Domain=127.0.0.1; Path=/; HttpOnly; SameSite=Lax"
        ))
        .unwrap();
        browser.client.add_cookie(cookie).await.unwrap();
    }
}

async fn pathname(browser: &Browser) -> String {
    browser
        .client
        .execute("return location.pathname;", vec![])
        .await
        .unwrap()
        .as_str()
        .unwrap_or_default()
        .to_owned()
}

/// True when a header link navigates without a full page load, i.e. the
/// hydrated router is still alive.
async fn client_navigation_alive(browser: &Browser, from: &str) -> bool {
    let href = if from == "/propostas" {
        "/auditoria"
    } else {
        "/propostas"
    };
    browser
        .client
        .execute("window.__alive = true;", vec![])
        .await
        .unwrap();
    let Ok(link) = browser
        .client
        .find(Locator::Css(&format!("a[href='{href}']")))
        .await
    else {
        return false;
    };
    if link.click().await.is_err() {
        return false;
    }
    let start = std::time::Instant::now();
    while pathname(browser).await != href {
        if start.elapsed() > WAIT {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    browser
        .client
        .execute("return window.__alive === true;", vec![])
        .await
        .unwrap()
        .as_bool()
        .unwrap_or(false)
}

/// Regression: `/notificacoes` with unread notifications rendered a `<form>`
/// inside `<p>`; the HTML parser moved it, hydration panicked and the whole
/// app stopped (only on reload, so the screen "worked once").
#[tokio::test]
#[ignore = "requires built app and chromedriver; run with `just e2e`"]
async fn every_route_hydrates_and_keeps_working_for_each_role() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("e2e.db");
    seed_dev(&db_path);
    let (tokens, ids) = sessions(&db_path, &["ana@dev.invalid", "admin@dev.invalid"]).await;
    let app = App::start(&db_path, &[]);
    let browser = Browser::start().await;
    let mut problems = Vec::new();

    for (role, token) in [
        ("visitor", None),
        ("ana", Some(&tokens[0])),
        ("admin", Some(&tokens[1])),
    ] {
        set_session(&browser, &app, token).await;
        let _ = browser.console().await;
        for (n, id) in ids.iter().take(2).enumerate() {
            for route in ROUTES {
                if n > 0 && !route.contains("{id}") {
                    continue;
                }
                let path = route.replace("{id}", id);
                browser.goto_hydrated(&app.url(&path)).await;
                for line in browser.crashes().await {
                    problems.push(format!("{role} load {path}: {line}"));
                }
                if !client_navigation_alive(&browser, &path).await {
                    problems.push(format!("{role} {path}: client navigation dead"));
                }
            }
        }
        // Client-side navigation through the header, several rounds.
        browser.goto_hydrated(&app.url("/")).await;
        for round in 0..2 {
            for href in [
                "/propostas",
                "/dashboard",
                "/metaleis",
                "/setup",
                "/estatisticas",
                "/auditoria",
                "/",
            ] {
                if let Ok(link) = browser
                    .client
                    .find(Locator::Css(&format!("a[href='{href}']")))
                    .await
                {
                    link.click().await.unwrap();
                }
                tokio::time::sleep(Duration::from_millis(300)).await;
                for line in browser.crashes().await {
                    problems.push(format!("{role} nav#{round} {href}: {line}"));
                }
            }
        }
    }
    browser.close().await;
    for p in &problems {
        eprintln!("PROBLEM {p}");
    }
    assert!(problems.is_empty(), "{} problems", problems.len());
}

async fn fill(browser: &Browser, css: &str, text: &str) {
    let el = browser
        .client
        .wait()
        .at_most(WAIT)
        .for_element(Locator::Css(css))
        .await
        .expect(css);
    el.clear().await.unwrap();
    el.send_keys(text).await.unwrap();
}

async fn click(browser: &Browser, css: &str) {
    browser
        .client
        .wait()
        .at_most(WAIT)
        .for_element(Locator::Css(css))
        .await
        .expect(css)
        .click()
        .await
        .unwrap();
}

/// Regression: leaving the face step (face accepted, TOTP step shown)
/// disposed the face widget while its button still read the derived
/// `disabled` signal; the panic stopped the app mid sign-in, intermittently.
#[tokio::test]
#[ignore = "requires built app and chromedriver; run with `just e2e`"]
async fn face_step_to_totp_step_never_panics() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("e2e.db");
    seed_dev(&db_path);
    let app = App::start(&db_path, &[]);
    let browser = Browser::start().await;
    let mut problems = Vec::new();
    for round in 0..10 {
        let _ = browser.console().await;
        browser.goto_hydrated(&app.url("/login")).await;
        fill(&browser, "[data-testid=login-email]", "ana@dev.invalid").await;
        fill(
            &browser,
            "[data-testid=login-password]",
            "senha-sintetica-dev",
        )
        .await;
        click(&browser, "[data-testid=login-submit]").await;
        fill(&browser, "[data-testid=face-phrase]", "rosto-sintetico-dev").await;
        click(&browser, "[data-testid=face-synthetic]").await;
        browser
            .client
            .wait()
            .at_most(WAIT)
            .for_element(Locator::Css("[data-testid=login-code]"))
            .await
            .expect("TOTP step");
        tokio::time::sleep(Duration::from_millis(300)).await;
        for line in browser.crashes().await {
            problems.push(format!("round {round}: {line}"));
        }
    }
    browser.close().await;
    for p in &problems {
        eprintln!("PROBLEM {p}");
    }
    assert!(problems.is_empty(), "{} problems", problems.len());
}

#[tokio::test]
#[ignore = "requires built app and chromedriver; run with `just e2e`"]
async fn browser_panic_is_visible_logged_on_server_and_navigation_recovers() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("e2e.db");
    let log = dir.path().join("server.log");
    let app = App::start(
        &db_path,
        &[
            (STDERR_FILE, log.to_str().unwrap()),
            ("RUST_LOG", "dwc=info,tower_http=info"),
        ],
    );
    let browser = Browser::start().await;
    browser.goto_hydrated(&app.url("/")).await;
    // Debug builds export a deliberate panic (the same path as a hydration panic).
    browser
        .client
        .execute(
            "import(document.querySelector('link[rel=modulepreload]').href)
                .then(m => setTimeout(() => m.dwc_debug_panic(), 0));",
            vec![],
        )
        .await
        .unwrap();
    let notice = browser
        .wait_text("[data-testid=app-crashed]", |t| !t.is_empty())
        .await;
    assert!(notice.contains("erro interno"), "{notice}");

    // The router is gone, but links are plain anchors: a full load recovers.
    browser
        .client
        .find(Locator::Css("a[href='/propostas']"))
        .await
        .unwrap()
        .click()
        .await
        .unwrap();
    browser
        .client
        .wait()
        .at_most(WAIT)
        .for_element(Locator::Css("main[data-hydrated='true']"))
        .await
        .unwrap();
    assert_eq!(pathname(&browser).await, "/propostas");
    assert!(
        browser
            .client
            .find(Locator::Css("[data-testid=app-crashed]"))
            .await
            .is_err(),
        "fresh page has no crash notice"
    );
    assert!(client_navigation_alive(&browser, "/propostas").await);
    browser.close_allowing_crash().await;

    let start = std::time::Instant::now();
    let text = loop {
        let text = std::fs::read_to_string(&log).unwrap_or_default();
        if text.contains("dwc_debug_panic") || start.elapsed() > WAIT {
            break text;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    };
    assert!(
        text.contains("browser: panicked at") && text.contains("dwc_debug_panic"),
        "browser panic reported to the server log:\n{text}"
    );
    assert!(
        text.contains("request{id=") && text.contains("status=200"),
        "access log:\n{text}"
    );
}
