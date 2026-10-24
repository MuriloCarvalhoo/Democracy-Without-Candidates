//! Identity endpoints exercised through the real router (server functions are
//! public network endpoints): authorization, CSRF origin check and cookies.

use std::sync::Arc;

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use leptos::{prelude::LeptosOptions, server_fn::ServerFn};
use tower::ServiceExt;

use super::{AppState, IdentityServices, http::router};
use crate::{
    application::identity::{enroll::tests::enrolled, testing::World},
    features::{
        account::RegenerateCodes, login::LoginStart, profile::MyProfile, profile::SavePrivacy,
        reauth::RequestReauth, register::RegisterAccount,
    },
    infrastructure::{
        address::{AddressProvider, testing::mock_brasilapi},
        clock::ManualClock,
        config::Config,
        face::FaceVerifier,
        flags::Flags,
        secrets::SecretBox,
    },
};

const ORIGIN: &str = "http://127.0.0.1:3000";

async fn app(w: &World) -> axum::Router {
    let config = Config::from_lookup("127.0.0.1:3000".parse().unwrap(), |k| {
        match k {
            "DWC_ENV" => Some("test"),
            "DWC_DB_PATH" => Some("unused.db"),
            "DWC_COUNTRY" => Some("BR"),
            "DWC_IDENTITY_PROVIDER" => Some("synthetic"),
            _ => None,
        }
        .map(String::from)
    })
    .unwrap();
    let state = AppState {
        db: w.t.db.clone(),
        config: Arc::new(config),
        flags: Arc::new(Flags::default()),
        clock: Arc::new(ManualClock::new(w.now)),
        face: FaceVerifier::Synthetic,
        identity: Arc::new(IdentityServices {
            secrets: SecretBox::from_key(&[9; 32]),
            address: Some(AddressProvider::brasilapi(
                &mock_brasilapi().await,
                std::time::Duration::from_millis(500),
            )),
        }),
    };
    let options = LeptosOptions::builder()
        .output_name("dwc")
        .site_root("target/site")
        .build();
    router(state, options)
}

struct Reply {
    status: StatusCode,
    cookies: Vec<String>,
    body: String,
}

async fn call(
    app: &axum::Router,
    path: &str,
    body: &str,
    cookie: Option<&str>,
    origin: &str,
) -> Reply {
    let mut req = Request::post(path)
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header(header::ORIGIN, origin);
    if let Some(c) = cookie {
        req = req.header(header::COOKIE, c);
    }
    let res = app
        .clone()
        .oneshot(req.body(Body::from(body.to_owned())).unwrap())
        .await
        .unwrap();
    let status = res.status();
    let cookies = res
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .map(|v| v.to_str().unwrap().to_owned())
        .collect();
    let bytes = axum::body::to_bytes(res.into_body(), 1 << 20)
        .await
        .unwrap();
    Reply {
        status,
        cookies,
        body: String::from_utf8_lossy(&bytes).into_owned(),
    }
}

#[tokio::test]
async fn protected_endpoints_reject_missing_session_and_cross_origin() {
    let w = World::new().await;
    let app = app(&w).await;
    let r = call(&app, MyProfile::PATH, "", None, ORIGIN).await;
    assert!(!r.status.is_success());
    assert!(r.body.contains("Unauthenticated"), "{}", r.body);

    let r = call(
        &app,
        RequestReauth::PATH,
        "action=profile%3Aprivacy&resource=x&face_challenge=c&face_image_b64=YQ%3D%3D&totp_code=123456",
        None,
        "https://evil.example",
    )
    .await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn registration_sets_http_only_session_and_pending_cannot_act() {
    let w = World::new().await;
    let app = app(&w).await;
    let r = call(
        &app,
        RegisterAccount::PATH,
        "name=Ana+Rede&email=rede%40example.org&password=senha+de+teste+longa&cep=89010025",
        None,
        ORIGIN,
    )
    .await;
    assert!(r.status.is_success(), "{}", r.body);
    let cookie = r
        .cookies
        .iter()
        .find(|c| c.starts_with("dwc_session="))
        .expect("session cookie");
    assert!(cookie.contains("HttpOnly") && cookie.contains("SameSite=Lax"));
    assert!(!cookie.contains("Max-Age") && !cookie.contains("Domain"));
    let session = cookie.split(';').next().unwrap();

    // Pending account: profile readable, critical actions refused on the server.
    let r = call(&app, MyProfile::PATH, "", Some(session), ORIGIN).await;
    assert!(
        r.status.is_success() && r.body.contains("\"pending\":true"),
        "{}",
        r.body
    );
    let r = call(
        &app,
        RegenerateCodes::PATH,
        "password=x&face_challenge=c&face_image_b64=YQ%3D%3D&totp_code=123456",
        Some(session),
        ORIGIN,
    )
    .await;
    assert!(r.body.contains("Forbidden"), "{}", r.body);
}

#[tokio::test]
async fn privacy_change_needs_reauth_and_login_uses_auth_cookie() {
    let w = World::new().await;
    let app = app(&w).await;
    let e = enrolled(&w, "ana@example.org").await;
    let session = format!("dwc_session={}", e.session_token);
    let r = call(
        &app,
        SavePrivacy::PATH,
        "profile_visibility=Public&default_vote_visibility=Public",
        Some(&session),
        ORIGIN,
    )
    .await;
    assert!(r.body.contains("ReauthRequired"), "{}", r.body);

    let r = call(
        &app,
        LoginStart::PATH,
        "email=ana%40example.org&password=senha+de+teste+longa",
        None,
        ORIGIN,
    )
    .await;
    assert!(
        r.status.is_success() && r.body.contains("Face"),
        "{}",
        r.body
    );
    let auth = r
        .cookies
        .iter()
        .find(|c| c.starts_with("dwc_auth="))
        .expect("auth cookie");
    assert!(auth.contains("HttpOnly") && auth.contains("Max-Age=300"));
    assert!(
        !r.cookies.iter().any(|c| c.starts_with("dwc_session=")),
        "no session before face + TOTP"
    );
}
