//! Debate browser regression: a visitor reads a seeded debate, a seeded synthetic
//! citizen logs in through the UI (password + synthetic face + TOTP), comments
//! and edits; an admin removes the comment and visitors stop seeing it. `just e2e`.

mod support;

use dwc::{
    application::session::{self, SessionPolicy},
    infrastructure::{
        db::{Db, query_opt},
        fixtures::{DEV_FACE_PHRASE, DEV_PASSWORD, DEV_TOTP_SECRET},
        totp,
    },
};
use fantoccini::{Locator, cookies::Cookie, elements::Element};
use support::{App, Browser, WAIT, http_post, seed_dev};

const OWN_COMMENT: &str = "Comentário E2E <b>sem HTML</b>\nsegunda linha";
const EDITED: &str = "Comentário E2E editado: falta iluminação";

/// Seeded debate proposal id and a session token for the seeded admin.
async fn prepare(path: &std::path::Path) -> (String, String) {
    seed_dev(path);
    let db = Db::open(path).await.unwrap();
    let tx = db.write().await.unwrap();
    let proposal: String = query_opt(
        &tx,
        "SELECT id FROM proposals WHERE title LIKE '%Reforma da praça central%'",
        (),
    )
    .await
    .unwrap()
    .unwrap()
    .get(0)
    .unwrap();
    let admin: String = query_opt(
        &tx,
        "SELECT id FROM users WHERE email = 'admin@dev.invalid'",
        (),
    )
    .await
    .unwrap()
    .unwrap()
    .get(0)
    .unwrap();
    let token = session::create(
        &tx,
        chrono::Utc::now(),
        SessionPolicy::from_secs(900, 8 * 3600),
        &admin,
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    (proposal, token)
}

async fn fill(b: &Browser, css: &str, text: &str) {
    let el = b
        .client
        .wait()
        .at_most(WAIT)
        .for_element(Locator::Css(css))
        .await
        .expect(css);
    el.clear().await.unwrap();
    el.send_keys(text).await.unwrap();
}

async fn click(b: &Browser, css: &str) {
    b.client
        .wait()
        .at_most(WAIT)
        .for_element(Locator::Css(css))
        .await
        .expect(css)
        .click()
        .await
        .unwrap();
}

/// The list item of the comment whose text contains `needle`.
async fn comment_with(b: &Browser, needle: &str) -> Element {
    b.client
        .wait()
        .at_most(WAIT)
        .for_element(Locator::XPath(&format!(
            "//li[@data-testid='comment'][.//p[@data-testid='comment-text'][contains(., '{needle}')]]"
        )))
        .await
        .unwrap_or_else(|_| panic!("comment containing {needle}"))
}

async fn within(el: &Element, css: &str) -> Element {
    let start = std::time::Instant::now();
    loop {
        if let Ok(found) = el.find(Locator::Css(css)).await {
            return found;
        }
        assert!(start.elapsed() < WAIT, "timed out waiting for {css}");
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
}

async fn login_as_ana(b: &Browser, app: &App) {
    b.goto_hydrated(&app.url("/login")).await;
    fill(b, "[data-testid=login-email]", "ana@dev.invalid").await;
    fill(b, "[data-testid=login-password]", DEV_PASSWORD).await;
    click(b, "[data-testid=login-submit]").await;
    fill(b, "[data-testid=face-phrase]", DEV_FACE_PHRASE).await;
    click(b, "[data-testid=face-synthetic]").await;
    let secret = totp_rs::Secret::try_from_base32(DEV_TOTP_SECRET)
        .unwrap()
        .as_bytes()
        .to_vec();
    let code = totp::code_at(&secret, chrono::Utc::now().timestamp());
    fill(b, "[data-testid=login-code]", &code).await;
    click(b, "[data-testid=login-code-submit]").await;
    b.client
        .wait()
        .at_most(WAIT)
        .for_element(Locator::Css("[data-testid=nav-profile]"))
        .await
        .expect("signed in");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires built app and chromedriver; run with `just e2e`"]
async fn visitor_reads_citizen_comments_and_edits_admin_removes() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("e2e.db");
    let (id, admin_token) = prepare(&db_path).await;
    let app = App::start(&db_path, &[]);
    let b = Browser::start().await;
    let debate = app.url(&format!("/propostas/{id}/debate"));

    // Visitor: proposal detail links to the debate (client-side navigation).
    b.goto_hydrated(&app.url(&format!("/propostas/{id}"))).await;
    b.client
        .execute("window.__noReload = true;", vec![])
        .await
        .unwrap();
    click(&b, "[data-testid=debate-link]").await;
    comment_with(&b, "bancos com sombra").await;
    assert_eq!(
        b.client
            .execute("return window.__noReload === true;", vec![])
            .await
            .unwrap(),
        serde_json::Value::Bool(true)
    );
    let seeded = b
        .client
        .find_all(Locator::Css("[data-testid=comment]"))
        .await
        .unwrap();
    assert_eq!(seeded.len(), 4, "seeded thread with replies");
    assert!(
        b.text_of("[data-testid=debate-block]")
            .await
            .contains("Entre na sua conta")
    );
    assert!(
        b.client
            .find(Locator::Css("[data-testid=comment-body]"))
            .await
            .is_err(),
        "visitors get no composer"
    );
    assert!(
        b.text_of("[data-testid=amendments-link]")
            .await
            .contains("(1)")
    );

    // Citizen of the jurisdiction logs in through the UI and comments.
    login_as_ana(&b, &app).await;
    b.goto_hydrated(&debate).await;
    fill(&b, "[data-testid=comment-body]", OWN_COMMENT).await;
    click(&b, "[data-testid=comment-submit]").await;
    let own = comment_with(&b, "sem HTML").await;
    let text = within(&own, "[data-testid=comment-text]").await;
    assert_eq!(
        text.text().await.unwrap(),
        OWN_COMMENT,
        "plain text with line break, markup shown literally"
    );
    assert!(
        text.find(Locator::Css("b")).await.is_err(),
        "user markup is never parsed as HTML"
    );
    assert_eq!(
        b.client
            .find(Locator::Css("[data-testid=comment-body]"))
            .await
            .unwrap()
            .prop("value")
            .await
            .unwrap()
            .unwrap_or_default(),
        "",
        "composer cleared after the server confirmed"
    );

    // Edit the own comment: shows "editado".
    within(&own, "[data-testid=comment-edit]")
        .await
        .click()
        .await
        .unwrap();
    fill(&b, "[data-testid=edit-body]", EDITED).await;
    click(&b, "[data-testid=edit-submit]").await;
    let own = comment_with(&b, "falta iluminação").await;
    assert_eq!(
        within(&own, "[data-testid=comment-edited]")
            .await
            .text()
            .await
            .unwrap(),
        "editado"
    );

    // Report another citizen's comment (once), then suggest an amendment.
    let other = comment_with(&b, "custo estimado").await;
    within(&other, "[data-testid=comment-report]")
        .await
        .click()
        .await
        .unwrap();
    within(&other, "[data-testid=report-submit]")
        .await
        .click()
        .await
        .unwrap();
    b.wait_text("[data-testid=debate-page] div[aria-live]", |t| {
        t.contains("Denúncia registrada")
    })
    .await;
    // After the refetch the comment shows the report state, without the button.
    let other = b
        .client
        .wait()
        .at_most(WAIT)
        .for_element(Locator::XPath(
            "//li[@data-testid='comment'][.//p[contains(., 'custo estimado')]][.//p[contains(., 'Você denunciou')]]",
        ))
        .await
        .expect("reported state");
    assert!(
        other
            .find(Locator::Css("[data-testid=comment-report]"))
            .await
            .is_err(),
        "one report per account"
    );
    click(&b, "[data-testid=amendments-link]").await;
    fill(&b, "[data-testid=amendment-title]", "Emenda E2E").await;
    fill(
        &b,
        "[data-testid=amendment-body]",
        "Art. 3º Iluminação em LED em toda a praça.",
    )
    .await;
    click(&b, "[data-testid=amendment-submit]").await;
    b.wait_text("[data-testid=amendment-list]", |t| t.contains("Emenda E2E"))
        .await;
    assert_eq!(
        b.client
            .find_all(Locator::Css("[data-testid=amendment]"))
            .await
            .unwrap()
            .len(),
        2
    );

    // Admin session (seeded admin, injected cookie): report queue, then removal
    // of the citizen's comment with a reason.
    b.client.delete_all_cookies().await.unwrap();
    let cookie = Cookie::parse(format!(
        "dwc_session={admin_token}; Domain=127.0.0.1; Path=/; HttpOnly; SameSite=Lax"
    ))
    .unwrap();
    b.client.add_cookie(cookie).await.unwrap();
    b.goto_hydrated(&app.url("/admin/moderacao")).await;
    b.wait_text("[data-testid=reported-comment]", |t| {
        t.contains("custo estimado") && t.contains("Spam")
    })
    .await;
    b.goto_hydrated(&debate).await;
    let target = comment_with(&b, "falta iluminação").await;
    within(&target, "[data-testid=moderate-toggle]")
        .await
        .click()
        .await
        .unwrap();
    fill(&b, "[data-testid=moderate-reason]", "Remoção de teste E2E").await;
    click(&b, "[data-testid=moderate-remove]").await;
    let target = b
        .client
        .wait()
        .at_most(WAIT)
        .for_element(Locator::XPath(
            "//li[@data-testid='comment'][.//p[@data-testid='comment-removed']][.//p[contains(., 'falta iluminação')]]",
        ))
        .await
        .expect("moderator sees the removed comment with its kept content");
    within(&target, "[data-testid=moderate-toggle]")
        .await
        .click()
        .await
        .unwrap();
    within(&target, "[data-testid=moderate-restore]").await;

    // Visitor: placeholder only, the content is gone from the public view.
    b.client.delete_all_cookies().await.unwrap();
    b.goto_hydrated(&debate).await;
    comment_with(&b, "bancos com sombra").await;
    b.wait_text("[data-testid=comment-list]", |t| {
        t.contains("Comentário removido pela moderação.")
    })
    .await;
    let page = b.text_of("main").await;
    assert!(
        !page.contains("falta iluminação") && !page.contains("sem HTML"),
        "removed content hidden"
    );
    b.close().await;
}

/// Mutating debate endpoints reject cross-origin requests and missing sessions.
#[tokio::test]
#[ignore = "requires built app; run with `just e2e`"]
async fn debate_endpoints_require_session_and_same_origin() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("e2e.db");
    let (id, _) = prepare(&db_path).await;
    let app = App::start(&db_path, &[]);
    let origin = app.url("");
    let body = format!("id={id}&body=Ol%C3%A1");
    let form = ("Content-Type", "application/x-www-form-urlencoded");
    let (status, _) = http_post(
        app.port,
        "/api/debate/comment",
        &[("Origin", "https://evil.example"), form],
        &body,
    );
    assert_eq!(status, 403);
    for path in [
        "/api/debate/comment",
        "/api/debate/lock",
        "/api/debate/amend",
    ] {
        let (status, reply) = http_post(
            app.port,
            path,
            &[("Origin", origin.as_str()), form],
            &format!("{body}&locked=true&reason=motivo+longo&title=T%C3%ADtulo"),
        );
        assert_ne!(status, 200, "{path}");
        assert!(reply.contains("Unauthenticated"), "{path}: {reply}");
    }
    let (status, reply) = http_post(
        app.port,
        "/api/debate/thread",
        &[("Origin", origin.as_str()), form],
        &format!("id={id}"),
    );
    assert_eq!(status, 200, "public read: {reply}");
    assert!(reply.contains("bancos com sombra"));
}
