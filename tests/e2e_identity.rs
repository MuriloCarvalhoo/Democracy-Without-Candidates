//! Identity browser regression: register (mocked BrasilAPI), enroll synthetic face
//! + TOTP, logout, login with every factor and idle session expiry. `just e2e`.

mod support;

use std::time::Duration;

use axum::{Router, extract::Path, http::StatusCode, response::IntoResponse, routing::get};
use dwc::infrastructure::{db::Db, flags, migrations, totp};
use fantoccini::Locator;
use support::{App, Browser, WAIT, seed_dev};

const IDLE_SECS: u64 = 12;

/// Local BrasilAPI stand-in: the app never reaches the network in tests.
async fn mock_brasilapi() -> String {
    async fn cep(Path(cep): Path<String>) -> axum::response::Response {
        let body = match cep.as_str() {
            "89010025" => {
                serde_json::json!({"cep": "89010025", "state": "SC", "city": "Blumenau", "neighborhood": "Centro", "street": "Rua Doutor Luiz de Freitas Melro", "service": "open-cep", "ibge": {"city": "4202404", "state": "42"}})
            }
            "80010000" => {
                // Slow answer for another CEP: must be ignored once the CEP changes.
                tokio::time::sleep(Duration::from_millis(1500)).await;
                serde_json::json!({"cep": "80010000", "state": "PR", "city": "Curitiba", "neighborhood": "Centro", "street": "Praça Tiradentes", "service": "open-cep", "ibge": {"city": "4106902", "state": "41"}})
            }
            _ => return StatusCode::NOT_FOUND.into_response(),
        };
        axum::Json(body).into_response()
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, Router::new().route("/api/cep/v1/{cep}", get(cep))).await
    });
    format!("http://{addr}")
}

/// Migrated database with the BrasilAPI provider flag on (read at app startup).
async fn prepare_db(path: &std::path::Path) {
    let db = Db::open(path).await.unwrap();
    migrations::run(&db).await.unwrap();
    flags::store(&db, flags::ADDRESS_BRASILAPI, true, Some("e2e"))
        .await
        .unwrap();
}

/// Next TOTP code whose step is newer than the last one used (replay is blocked).
fn next_code(secret: &[u8], last_step: &mut i64) -> String {
    let now = chrono::Utc::now().timestamp();
    let step = (*last_step + 1).max(now / 30);
    assert!(
        step <= now / 30 + 1,
        "would need to wait for the next TOTP step"
    );
    *last_step = step;
    totp::code_at(secret, step * 30)
}

async fn fill(b: &Browser, css: &str, text: &str) {
    let el = b.client.find(Locator::Css(css)).await.expect(css);
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

async fn wait_for(b: &Browser, css: &str) {
    b.client
        .wait()
        .at_most(WAIT)
        .for_element(Locator::Css(css))
        .await
        .expect(css);
}

async fn use_synthetic_face(b: &Browser) {
    wait_for(b, "[data-testid=face-phrase]").await;
    fill(b, "[data-testid=face-phrase]", "rosto-e2e").await;
    click(b, "[data-testid=face-synthetic]").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires built app and chromedriver; run with `just e2e`"]
async fn register_enroll_logout_login_and_idle_expiry() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("e2e.db");
    prepare_db(&db_path).await;
    let mock = mock_brasilapi().await;
    let idle = IDLE_SECS.to_string();
    let app = App::start(
        &db_path,
        &[
            ("DWC_BRASILAPI_URL", &mock),
            ("DWC_SESSION_IDLE_SECS", &idle),
        ],
    );
    let b = Browser::start().await;

    // Registration with server-side address lookup; stale responses are ignored.
    b.goto_hydrated(&app.url("/cadastro")).await;
    fill(&b, "[data-testid=register-name]", "Ana E2E").await;
    fill(&b, "[data-testid=register-email]", "ana.e2e@example.org").await;
    fill(&b, "[data-testid=register-password]", "senha e2e bem longa").await;
    fill(&b, "[data-testid=register-cep]", "80010000").await;
    click(&b, "[data-testid=cep-lookup]").await;
    fill(&b, "[data-testid=register-cep]", "89010-025").await;
    click(&b, "[data-testid=cep-lookup]").await;
    b.wait_text("[data-testid=address-city]", |t| t == "Blumenau / SC")
        .await;
    tokio::time::sleep(Duration::from_millis(2000)).await;
    assert_eq!(
        b.text_of("[data-testid=address-city]").await,
        "Blumenau / SC",
        "late answer for the previous CEP must not replace the address"
    );
    click(&b, "[data-testid=register-submit]").await;

    // Enrollment: synthetic face, then TOTP from the secret shown on the page.
    use_synthetic_face(&b).await;
    let secret_b32 = b
        .wait_text("[data-testid=totp-secret]", |t| t.len() >= 32)
        .await;
    let secret = totp_rs::Secret::try_from_base32(&secret_b32)
        .unwrap()
        .as_bytes()
        .to_vec();
    let mut last_step = 0;
    fill(
        &b,
        "[data-testid=totp-code]",
        &next_code(&secret, &mut last_step),
    )
    .await;
    click(&b, "[data-testid=totp-confirm]").await;
    wait_for(&b, "[data-testid=recovery-codes]").await;
    let codes = b
        .client
        .find_all(Locator::Css("[data-testid=recovery-codes] li"))
        .await
        .unwrap();
    assert_eq!(codes.len(), 10);
    click(&b, "[data-testid=enroll-continue]").await;
    b.wait_text("[data-testid=profile-city]", |t| t == "Blumenau / SC")
        .await;

    // Logout, then login with password + face + TOTP.
    click(&b, "[data-testid=nav-logout]").await;
    wait_for(&b, "[data-testid=nav-login]").await;
    b.goto_hydrated(&app.url("/login")).await;
    fill(&b, "[data-testid=login-email]", "ANA.E2E@example.org").await;
    fill(&b, "[data-testid=login-password]", "senha e2e bem longa").await;
    click(&b, "[data-testid=login-submit]").await;
    use_synthetic_face(&b).await;
    wait_for(&b, "[data-testid=login-code]").await;
    fill(
        &b,
        "[data-testid=login-code]",
        &next_code(&secret, &mut last_step),
    )
    .await;
    click(&b, "[data-testid=login-code-submit]").await;
    wait_for(&b, "[data-testid=nav-profile]").await;
    b.goto_hydrated(&app.url("/perfil")).await;
    b.wait_text("[data-testid=profile-name]", |t| t == "Ana E2E")
        .await;

    // Idle expiry decided by the server.
    tokio::time::sleep(Duration::from_secs(IDLE_SECS + 2)).await;
    b.goto_hydrated(&app.url("/perfil")).await;
    b.wait_text(".notice.error", |t| t.contains("expirou"))
        .await;
    wait_for(&b, "[data-testid=nav-login]").await;
    b.close().await;
}

/// `DWC_DEV_SKIP_TOTP=1`: password + face sign in without the TOTP step.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires built app and chromedriver; run with `just e2e`"]
async fn dev_skip_totp_signs_in_after_face() {
    use dwc::infrastructure::fixtures::{DEV_FACE_PHRASE, DEV_PASSWORD};
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("e2e.db");
    seed_dev(&db_path);
    let app = App::start(&db_path, &[("DWC_DEV_SKIP_TOTP", "1")]);
    let b = Browser::start().await;
    b.goto_hydrated(&app.url("/login")).await;
    wait_for(&b, "[data-testid=skip-totp-banner]").await;
    fill(&b, "[data-testid=login-email]", "ana@dev.invalid").await;
    fill(&b, "[data-testid=login-password]", DEV_PASSWORD).await;
    click(&b, "[data-testid=login-submit]").await;
    wait_for(&b, "[data-testid=face-phrase]").await;
    fill(&b, "[data-testid=face-phrase]", DEV_FACE_PHRASE).await;
    click(&b, "[data-testid=face-synthetic]").await;
    wait_for(&b, "[data-testid=nav-profile]").await;
    assert!(
        b.client
            .find(Locator::Css("[data-testid=login-code]"))
            .await
            .is_err(),
        "no TOTP step"
    );
    b.close().await;
}

/// Real-provider UX path: camera (Chrome fake device) → JPEG → server. The face
/// engine integration is a placeholder, so the server answers "unavailable" and the
/// account stays pending: a failed capture is never turned into a validation.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires built app and chromedriver; run with `just e2e`"]
async fn camera_capture_reaches_server_and_failure_keeps_account_pending() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("e2e.db");
    prepare_db(&db_path).await;
    let mock = mock_brasilapi().await;
    // Unreachable engine: startup only warns, enrollment must fail closed.
    let face_key = dir.path().join("face.key");
    std::fs::write(&face_key, "k".repeat(40)).unwrap();
    let app = App::start(
        &db_path,
        &[
            ("DWC_BRASILAPI_URL", &mock),
            ("DWC_IDENTITY_PROVIDER", "face-service"),
            ("DWC_FACE_SERVICE_URL", "http://127.0.0.1:9"),
            ("DWC_FACE_SERVICE_KEY_FILE", face_key.to_str().unwrap()),
        ],
    );
    let b = Browser::start().await;
    b.goto_hydrated(&app.url("/cadastro")).await;
    fill(&b, "[data-testid=register-name]", "Bia Camera").await;
    fill(&b, "[data-testid=register-email]", "bia.camera@example.org").await;
    fill(&b, "[data-testid=register-password]", "senha e2e bem longa").await;
    fill(&b, "[data-testid=register-cep]", "89010025").await;
    click(&b, "[data-testid=cep-lookup]").await;
    b.wait_text("[data-testid=address-city]", |t| t == "Blumenau / SC")
        .await;
    click(&b, "[data-testid=register-submit]").await;

    click(&b, "[data-testid=camera-start]").await;
    wait_for(&b, "[data-testid=camera-capture]").await;
    tokio::time::sleep(Duration::from_millis(1000)).await;
    click(&b, "[data-testid=camera-capture]").await;
    b.wait_text(".notice.error", |t| t.contains("serviço facial"))
        .await;
    b.goto_hydrated(&app.url("/perfil")).await;
    wait_for(&b, "a[href='/cadastro/biometria']").await;
    b.close().await;
}
