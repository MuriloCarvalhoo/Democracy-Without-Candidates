//! Voting browser regressions (`just e2e`): a seeded synthetic account logs in
//! through the UI (password + synthetic face + TOTP), signs a proposal through
//! the reauth panel, casts a demonstrative public vote, sees "already voted" and
//! the result after the vote closes. Plus network-level rejections of the cast
//! endpoint. Synthetic identities only; no secret ballot exists in this build.

mod support;

use std::time::Duration;

use dwc::{
    application::{reauth, session},
    infrastructure::{
        clock::ts,
        db::{Db, query_opt},
        fixtures::{DEV_FACE_PHRASE, DEV_PASSWORD, DEV_TOTP_SECRET},
        totp,
    },
};
use fantoccini::Locator;
use support::{App, Browser, WAIT, http_post, seed_dev};

async fn proposal_id(db: &Db, title_like: &str) -> String {
    query_opt(
        &db.read().await.unwrap(),
        "SELECT id FROM proposals WHERE title LIKE ?1",
        [format!("%{title_like}%")],
    )
    .await
    .unwrap()
    .unwrap_or_else(|| panic!("seeded proposal {title_like}"))
    .get(0)
    .unwrap()
}

/// Next TOTP code with a step newer than the last one used (replay is blocked),
/// waiting for the clock when the window would be exceeded.
async fn next_code(last_step: &mut i64) -> String {
    let secret = totp_rs::Secret::try_from_base32(DEV_TOTP_SECRET)
        .unwrap()
        .as_bytes()
        .to_vec();
    loop {
        let now = chrono::Utc::now().timestamp() / 30;
        let step = (*last_step + 1).max(now);
        if step <= now + 1 {
            *last_step = step;
            return totp::code_at(&secret, step * 30);
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
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

async fn synthetic_face(b: &Browser) {
    fill(b, "[data-testid=face-phrase]", DEV_FACE_PHRASE).await;
    click(b, "[data-testid=face-synthetic]").await;
}

async fn reauth(b: &Browser, last_step: &mut i64) {
    synthetic_face(b).await;
    b.wait_text("[data-testid=face-captured]", |t| !t.is_empty())
        .await;
    fill(b, "[data-testid=factor-totp]", &next_code(last_step).await).await;
    click(b, "[data-testid=reauth-submit]").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires built app and chromedriver; run with `just e2e`"]
async fn citizen_signs_votes_publicly_and_sees_result_after_closing() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("e2e.db");
    seed_dev(&db_path);
    let (collecting, voting) = {
        let db = Db::open(&db_path).await.unwrap();
        (
            proposal_id(&db, "Ciclovia").await,
            proposal_id(&db, "Plano emergencial").await,
        )
    };
    let mut app = App::start(&db_path, &[("DWC_TICKER_SECS", "1")]);
    let b = Browser::start().await;
    let mut last_step = 0;

    // Login with every factor of the seeded synthetic account.
    b.goto_hydrated(&app.url("/login")).await;
    fill(&b, "[data-testid=login-email]", "ana@dev.invalid").await;
    fill(&b, "[data-testid=login-password]", DEV_PASSWORD).await;
    click(&b, "[data-testid=login-submit]").await;
    synthetic_face(&b).await;
    fill(
        &b,
        "[data-testid=login-code]",
        &next_code(&mut last_step).await,
    )
    .await;
    click(&b, "[data-testid=login-code-submit]").await;
    b.wait_text("[data-testid=nav-profile]", |t| !t.is_empty())
        .await;

    // Sign a proposal in collection through the reauth panel.
    b.goto_hydrated(&app.url(&format!("/propostas/{collecting}")))
        .await;
    b.wait_text("[data-testid=signature-count]", |t| t.starts_with("0 de 1"))
        .await;
    click(&b, "[data-testid=sign]").await;
    reauth(&b, &mut last_step).await;
    b.wait_text("[data-testid=signature-count]", |t| t.starts_with("1 de 1"))
        .await;

    // Vote: private is unavailable; consent to a public vote is required.
    b.goto_hydrated(&app.url(&format!("/propostas/{voting}/votar")))
        .await;
    assert!(
        b.wait_text("[data-testid=vote-private-unavailable]", |t| !t.is_empty())
            .await
            .contains("bloqueado")
    );
    click(&b, "[data-testid=vote-choice-yes]").await;
    click(&b, "[data-testid=vote-continue]").await;
    assert!(
        b.wait_text(".notice.error", |t| !t.is_empty())
            .await
            .contains("consentir"),
        "no consent, no vote"
    );
    click(&b, "[data-testid=vote-consent]").await;
    click(&b, "[data-testid=vote-continue]").await;
    assert!(
        b.client
            .find(Locator::Css("[data-testid=vote-confirmed]"))
            .await
            .is_err(),
        "nothing confirmed before the server accepts the ballot"
    );
    reauth(&b, &mut last_step).await;
    let confirmed = b
        .wait_text("[data-testid=vote-confirmed]", |t| t.contains("SIM"))
        .await;
    assert!(confirmed.contains("registrado"), "{confirmed}");

    // Second attempt: the page shows the stored ballot instead of the form.
    b.goto_hydrated(&app.url(&format!("/propostas/{voting}/votar")))
        .await;
    assert!(
        b.wait_text("[data-testid=vote-already]", |t| t.contains("SIM"))
            .await
            .contains("já registrado")
    );
    assert!(
        b.client
            .find(Locator::Css("[data-testid=vote-continue]"))
            .await
            .is_err()
    );

    // While open: no partial counts.
    b.goto_hydrated(&app.url(&format!("/propostas/{voting}/resultado")))
        .await;
    b.wait_text("[data-testid=result-open]", |t| !t.is_empty())
        .await;
    assert!(
        b.client
            .find(Locator::Css("[data-testid=result-yes]"))
            .await
            .is_err()
    );

    // Force the deadline with the app stopped, restart: the ticker closes the vote.
    app.stop();
    {
        let db = Db::open(&db_path).await.unwrap();
        let tx = db.write().await.unwrap();
        tx.execute(
            "UPDATE proposals SET vote_ends_at = ?2 WHERE id = ?1",
            (voting.as_str(), ts(chrono::Utc::now())),
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
    }
    app.resume();
    tokio::time::sleep(Duration::from_secs(2)).await;
    b.goto_hydrated(&app.url(&format!("/propostas/{voting}/resultado")))
        .await;
    // Blumenau electorate: 12 synthetic + ana + bruno; urgent quorum ceil(20% × 14) = 3.
    assert!(
        b.wait_text("[data-testid=result-outcome]", |t| !t.is_empty())
            .await
            .contains("Quórum não atingido")
    );
    assert_eq!(b.text_of("[data-testid=result-yes]").await, "1");
    assert_eq!(b.text_of("[data-testid=result-electorate]").await, "14");
    assert!(
        b.text_of("[data-testid=result-quorum]")
            .await
            .contains("não atingido")
    );

    // Seeded closed vote and statistics are public.
    b.goto_hydrated(&app.url("/estatisticas")).await;
    b.wait_text("[data-testid=stats-votes]", |t| t.contains("Iluminação"))
        .await;
    b.close().await;
}

/// The cast endpoint is a public network endpoint: no session, no reauth grant,
/// closed vote, missing consent and cross-origin calls are rejected server-side.
#[tokio::test]
#[ignore = "requires built app; run with `just e2e`"]
async fn cast_endpoint_rejects_without_session_reauth_consent_or_after_close() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("e2e.db");
    seed_dev(&db_path);
    let (voting, closed, token) = {
        let db = Db::open(&db_path).await.unwrap();
        let voting = proposal_id(&db, "Plano emergencial").await;
        let closed = proposal_id(&db, "Iluminação").await;
        let ana: String = query_opt(
            &db.read().await.unwrap(),
            "SELECT id FROM users WHERE email = 'ana@dev.invalid'",
            (),
        )
        .await
        .unwrap()
        .unwrap()
        .get(0)
        .unwrap();
        let now = chrono::Utc::now();
        let tx = db.write().await.unwrap();
        let token = session::create(
            &tx,
            now,
            session::SessionPolicy::from_secs(900, 28800),
            &ana,
        )
        .await
        .unwrap();
        let hash = session::token_hash(&token);
        reauth::grant(&tx, now, &hash, "proposal:vote", &closed)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        (voting, closed, token)
    };
    let app = App::start(&db_path, &[]);
    let origin = app.url("");
    let cookie = format!("dwc_session={token}");
    let form = |with_cookie: bool| {
        let mut h = vec![
            ("Origin", origin.as_str()),
            ("Content-Type", "application/x-www-form-urlencoded"),
        ];
        if with_cookie {
            h.push(("Cookie", cookie.as_str()));
        }
        h
    };
    let body = |id: &str, consent: bool, key: &str| {
        format!("id={id}&choice=yes&public_consent={consent}&idempotency_key={key}")
    };
    let cast = |headers: Vec<(&str, &str)>, body: String| {
        http_post(app.port, "/api/voting/cast", &headers, &body)
    };

    let cross = http_post(
        app.port,
        "/api/voting/cast",
        &[
            ("Origin", "https://evil.example"),
            ("Content-Type", "application/x-www-form-urlencoded"),
        ],
        &body(&voting, true, "attempt-cross"),
    );
    assert_eq!(cross.0, 403);
    let (status, text) = cast(form(false), body(&voting, true, "attempt-anon"));
    assert!(
        status != 200 && text.contains("Unauthenticated"),
        "{status} {text}"
    );
    let (status, text) = cast(form(true), body(&voting, false, "attempt-noconsent"));
    assert!(
        status != 200 && text.contains("public_consent"),
        "{status} {text}"
    );
    let (status, text) = cast(form(true), body(&voting, true, "attempt-noreauth"));
    assert!(
        status != 200 && text.contains("ReauthRequired"),
        "{status} {text}"
    );
    let (status, text) = cast(form(true), body(&closed, true, "attempt-closed"));
    assert!(
        status != 200 && text.contains("PhaseClosed"),
        "{status} {text}"
    );
    let (status, text) = cast(
        form(true),
        format!("id={voting}&choice=maybe&public_consent=true"),
    );
    assert!(status != 200 && text.contains("choice"), "{status} {text}");
}
