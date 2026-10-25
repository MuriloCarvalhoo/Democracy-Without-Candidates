//! IAM browser regression (`just e2e`): a newly registered citizen gets only the
//! `Cidadao` policy and cannot open the administration; the seeded master admin
//! creates a customer policy granting moderation, attaches it to the citizen
//! (each change after face reauthentication), the citizen then opens the
//! moderation queue, and detaching removes access on the next request.
//! Synthetic identities, `DWC_DEV_SKIP_TOTP=1`, mocked BrasilAPI.
//! Screenshots and console logs go to `artifacts/iam/` (gitignored).

mod support;

use std::time::Duration;

use axum::{Router, extract::Path, http::StatusCode, response::IntoResponse, routing::get};
use dwc::infrastructure::fixtures::{DEV_FACE_PHRASE, DEV_PASSWORD};
use fantoccini::Locator;
use support::{App, Browser, WAIT, seed_dev};

const CITIZEN_EMAIL: &str = "nova.iam@example.org";
const CITIZEN_FACE: &str = "rosto-iam-e2e";
const POLICY: &str = "ModeracaoE2E";
const MODERATION_DOC: &str = r#"{"Version":"2026-10-25","Statement":[{"Sid":"Moderar","Effect":"Allow","Action":"moderation:act","Resource":"*"}]}"#;

async fn mock_brasilapi() -> String {
    async fn cep(Path(cep): Path<String>) -> axum::response::Response {
        if cep != "89010025" {
            return StatusCode::NOT_FOUND.into_response();
        }
        axum::Json(serde_json::json!({"cep": "89010025", "state": "SC", "city": "Blumenau", "neighborhood": "Centro", "street": "Rua Doutor Luiz de Freitas Melro", "service": "open-cep", "ibge": {"city": "4202404", "state": "42"}})).into_response()
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, Router::new().route("/api/cep/v1/{cep}", get(cep))).await
    });
    format!("http://{addr}")
}

async fn wait_for(b: &Browser, css: &str) {
    b.client
        .wait()
        .at_most(WAIT)
        .for_element(Locator::Css(css))
        .await
        .expect(css);
}

async fn fill(b: &Browser, css: &str, text: &str) {
    wait_for(b, css).await;
    let el = b.client.find(Locator::Css(css)).await.expect(css);
    el.clear().await.unwrap();
    el.send_keys(text).await.unwrap();
}

async fn click(b: &Browser, css: &str) {
    wait_for(b, css).await;
    b.client
        .find(Locator::Css(css))
        .await
        .expect(css)
        .click()
        .await
        .unwrap();
}

async fn absent(b: &Browser, css: &str) -> bool {
    b.client.find(Locator::Css(css)).await.is_err()
}

/// Polls until `css` disappears (bounded).
async fn wait_gone(b: &Browser, css: &str) {
    let start = std::time::Instant::now();
    while !absent(b, css).await {
        assert!(start.elapsed() < WAIT, "{css} still present");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

async fn shot(b: &Browser, name: &str) {
    std::fs::create_dir_all("artifacts/iam").unwrap();
    let png = b.client.screenshot().await.unwrap();
    std::fs::write(format!("artifacts/iam/{name}.png"), png).unwrap();
}

/// Records console errors; only network failures of denied requests are expected.
async fn check_console(b: &Browser, name: &str) {
    let errors = b.console_errors().await;
    std::fs::create_dir_all("artifacts/iam").unwrap();
    std::fs::write(
        format!("artifacts/iam/{name}-console.txt"),
        errors.join("\n"),
    )
    .unwrap();
    let unexpected: Vec<&String> = errors
        .iter()
        .filter(|e| !e.contains("Failed to load resource"))
        .collect();
    assert!(unexpected.is_empty(), "console errors: {unexpected:?}");
}

/// Face reauthentication (TOTP skipped by `DWC_DEV_SKIP_TOTP`).
async fn confirm_face(b: &Browser, phrase: &str) {
    fill(b, "[data-testid=face-phrase]", phrase).await;
    click(b, "[data-testid=face-synthetic]").await;
    wait_for(b, "[data-testid=face-captured]").await;
    click(b, "[data-testid=reauth-submit]").await;
}

/// Reauthenticates and waits for the change to be applied.
async fn reauth(b: &Browser, phrase: &str) {
    confirm_face(b, phrase).await;
    wait_for(b, "[data-testid=iam-done]").await;
}

/// Moderation queue reachable: queue or its empty state, never the denial.
async fn moderation_allowed(b: &Browser, app: &App) {
    b.goto_hydrated(&app.url("/admin/moderacao")).await;
    wait_for(b, "[data-testid=moderation-queue], .notice.empty").await;
    assert!(absent(b, "[data-testid=admin-denied]").await);
}

async fn denied(b: &Browser, app: &App, path: &str) {
    b.goto_hydrated(&app.url(path)).await;
    let text = b
        .wait_text("[data-testid=admin-denied]", |t| !t.is_empty())
        .await;
    assert!(text.contains("permissão"), "{path}: {text}");
}

async fn register_and_enroll(b: &Browser, app: &App) {
    b.goto_hydrated(&app.url("/cadastro")).await;
    fill(b, "[data-testid=register-name]", "Nova Cidadã IAM").await;
    fill(b, "[data-testid=register-email]", CITIZEN_EMAIL).await;
    fill(b, "[data-testid=register-password]", "senha e2e bem longa").await;
    fill(b, "[data-testid=register-cep]", "89010-025").await;
    click(b, "[data-testid=cep-lookup]").await;
    b.wait_text("[data-testid=address-city]", |t| t == "Blumenau / SC")
        .await;
    click(b, "[data-testid=register-submit]").await;
    fill(b, "[data-testid=face-phrase]", CITIZEN_FACE).await;
    click(b, "[data-testid=face-synthetic]").await;
    wait_for(b, "[data-testid=totp-secret]").await;
    fill(b, "[data-testid=totp-code]", "000000").await;
    click(b, "[data-testid=totp-confirm]").await;
    click(b, "[data-testid=enroll-continue]").await;
    b.wait_text("[data-testid=profile-city]", |t| t == "Blumenau / SC")
        .await;
}

async fn login_master(b: &Browser, app: &App) {
    b.goto_hydrated(&app.url("/login")).await;
    fill(b, "[data-testid=login-email]", "master@dev.invalid").await;
    fill(b, "[data-testid=login-password]", DEV_PASSWORD).await;
    click(b, "[data-testid=login-submit]").await;
    fill(b, "[data-testid=face-phrase]", DEV_FACE_PHRASE).await;
    click(b, "[data-testid=face-synthetic]").await;
    wait_for(b, "[data-testid=nav-profile]").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires built app and chromedriver; run with `just e2e`"]
async fn master_grants_and_revokes_moderation_through_a_customer_policy() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("e2e.db");
    seed_dev(&db_path);
    let mock = mock_brasilapi().await;
    let app = App::start(
        &db_path,
        &[("DWC_BRASILAPI_URL", &mock), ("DWC_DEV_SKIP_TOTP", "1")],
    );

    // A newly registered citizen sees no administration.
    let citizen = Browser::start().await;
    register_and_enroll(&citizen, &app).await;
    assert!(absent(&citizen, "[data-testid=nav-admin]").await);
    for path in ["/admin", "/admin/iam", "/admin/moderacao"] {
        denied(&citizen, &app, path).await;
    }
    shot(&citizen, "01-citizen-denied").await;
    check_console(&citizen, "01-citizen-denied").await;

    // The master admin creates a customer policy granting moderation.
    let master = Browser::start().await;
    login_master(&master, &app).await;
    wait_for(&master, "[data-testid=nav-admin]").await;
    master.goto_hydrated(&app.url("/admin/iam/politicas")).await;
    wait_for(&master, "[data-testid=iam-policy-AdministratorAccess]").await;
    fill(&master, "[data-testid=iam-policy-name]", POLICY).await;
    fill(
        &master,
        "[data-testid=iam-policy-description]",
        "Moderação E2E",
    )
    .await;
    // Documents naming actions outside the catalog are refused with the reason.
    fill(
        &master,
        "[data-testid=iam-policy-document]",
        &MODERATION_DOC.replace("moderation:act", "moderation:delete_all"),
    )
    .await;
    click(&master, "[data-testid=iam-policy-create]").await;
    confirm_face(&master, DEV_FACE_PHRASE).await;
    master
        .wait_text(".notice.error", |t| t.contains("moderation:delete_all"))
        .await;
    shot(&master, "02a-policy-invalid").await;
    fill(&master, "[data-testid=iam-policy-document]", MODERATION_DOC).await;
    click(&master, "[data-testid=iam-policy-create]").await;
    reauth(&master, DEV_FACE_PHRASE).await;
    wait_for(&master, &format!("[data-testid=iam-policy-{POLICY}]")).await;
    shot(&master, "02-policy-created").await;

    // ... and attaches it to the citizen, who already holds Cidadao.
    master.goto_hydrated(&app.url("/admin/iam/usuarios")).await;
    fill(&master, "[data-testid=iam-user-query]", "nova.iam").await;
    click(&master, "[data-testid=iam-user-search]").await;
    click(&master, "[data-testid=iam-user-link]").await;
    wait_for(&master, "[data-testid=iam-user-policy-Cidadao]").await;
    wait_for(&master, "[data-testid=iam-attach-policy]").await;
    master
        .client
        .find(Locator::Css("[data-testid=iam-attach-policy]"))
        .await
        .unwrap()
        .select_by_label(POLICY)
        .await
        .unwrap();
    click(&master, "[data-testid=iam-attach-submit]").await;
    reauth(&master, DEV_FACE_PHRASE).await;
    let row = format!("[data-testid=iam-user-policy-{POLICY}]");
    wait_for(&master, &row).await;
    master
        .wait_text("[data-testid=iam-effective]", |t| {
            t.contains("moderation:act")
        })
        .await;
    shot(&master, "03-policy-attached").await;

    // Effective on the citizen's next request, without signing in again.
    moderation_allowed(&citizen, &app).await;
    wait_for(&citizen, "[data-testid=nav-admin]").await;
    denied(&citizen, &app, "/admin").await;
    moderation_allowed(&citizen, &app).await;
    shot(&citizen, "04-citizen-moderation").await;
    check_console(&citizen, "04-citizen-moderation").await;

    // Simulator shows the deciding statement.
    master.goto_hydrated(&app.url("/admin/iam/simulador")).await;
    fill(&master, "[data-testid=iam-sim-email]", CITIZEN_EMAIL).await;
    master
        .client
        .find(Locator::Css("[data-testid=iam-sim-action]"))
        .await
        .unwrap()
        .select_by_value("moderation:act")
        .await
        .unwrap();
    click(&master, "[data-testid=iam-sim-submit]").await;
    master
        .wait_text("[data-testid=iam-sim-verdict]", |t| t == "Permitido")
        .await;
    let reason = master.text_of("[data-testid=iam-sim-reason]").await;
    assert!(
        reason.contains(POLICY) && reason.contains("Moderar"),
        "{reason}"
    );
    shot(&master, "05-simulator").await;

    // Detaching removes access on the next request.
    master
        .goto_hydrated(&app.url("/admin/iam/usuarios?q=nova.iam"))
        .await;
    click(&master, "[data-testid=iam-user-link]").await;
    click(&master, &format!("{row} [data-testid=iam-detach]")).await;
    reauth(&master, DEV_FACE_PHRASE).await;
    wait_gone(&master, &row).await;
    shot(&master, "06-policy-detached").await;
    check_console(&master, "06-master").await;
    denied(&citizen, &app, "/admin/moderacao").await;
    shot(&citizen, "07-citizen-denied-again").await;

    citizen.close().await;
    master.close().await;
}

async fn simulate(b: &Browser, app: &App, email: &str, action: &str) -> (String, String) {
    b.goto_hydrated(&app.url("/admin/iam/simulador")).await;
    fill(b, "[data-testid=iam-sim-email]", email).await;
    b.client
        .find(Locator::Css("[data-testid=iam-sim-action]"))
        .await
        .unwrap()
        .select_by_value(action)
        .await
        .unwrap();
    click(b, "[data-testid=iam-sim-submit]").await;
    let verdict = b
        .wait_text("[data-testid=iam-sim-verdict]", |t| !t.is_empty())
        .await;
    (verdict, b.text_of("[data-testid=iam-sim-reason]").await)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires built app and chromedriver; run with `just e2e`"]
async fn group_membership_grants_its_policies() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("e2e.db");
    seed_dev(&db_path);
    let app = App::start(&db_path, &[("DWC_DEV_SKIP_TOTP", "1")]);
    let master = Browser::start().await;
    login_master(&master, &app).await;

    master.goto_hydrated(&app.url("/admin/iam/grupos")).await;
    fill(&master, "[data-testid=iam-group-name]", "Moderadores").await;
    click(&master, "[data-testid=iam-group-create]").await;
    reauth(&master, DEV_FACE_PHRASE).await;
    click(&master, "[data-testid=iam-groups] a").await;
    wait_for(&master, "[data-testid=iam-group-detail]").await;
    wait_for(&master, "[data-testid=iam-attach-policy]").await;
    master
        .client
        .find(Locator::Css("[data-testid=iam-attach-policy]"))
        .await
        .unwrap()
        .select_by_label("Moderador")
        .await
        .unwrap();
    click(&master, "[data-testid=iam-attach-submit]").await;
    reauth(&master, DEV_FACE_PHRASE).await;
    wait_for(&master, "[data-testid=iam-group-policy-Moderador]").await;
    fill(&master, "[data-testid=iam-member-email]", "ana@dev.invalid").await;
    click(&master, "[data-testid=iam-member-add]").await;
    reauth(&master, DEV_FACE_PHRASE).await;
    master
        .wait_text("[data-testid=iam-group-detail]", |t| {
            t.contains("ana@dev.invalid")
        })
        .await;
    shot(&master, "08-group").await;

    let (verdict, reason) = simulate(&master, &app, "ana@dev.invalid", "moderation:act").await;
    assert_eq!(verdict, "Permitido");
    assert!(reason.contains("Moderador"), "{reason}");
    let (verdict, _) = simulate(&master, &app, "ana@dev.invalid", "iam:manage").await;
    assert_eq!(verdict, "Negado");

    // Deleting the group removes the grant.
    master.goto_hydrated(&app.url("/admin/iam/grupos")).await;
    click(&master, "[data-testid=iam-groups] a").await;
    click(&master, "[data-testid=iam-group-delete]").await;
    // Back to the (now empty) list once the deletion is applied.
    confirm_face(&master, DEV_FACE_PHRASE).await;
    master.wait_text("h1", |t| t == "Grupos").await;
    wait_for(&master, ".notice.empty").await;
    let (verdict, _) = simulate(&master, &app, "ana@dev.invalid", "moderation:act").await;
    assert_eq!(verdict, "Negado");
    check_console(&master, "08-group").await;
    master.close().await;
}
