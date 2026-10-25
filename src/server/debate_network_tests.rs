//! Debate endpoints through the real router: server functions are public network
//! endpoints, so session, origin and territory are checked on the server.

use std::sync::Arc;

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use leptos::{prelude::LeptosOptions, server_fn::ServerFn};
use tower::ServiceExt;

use super::{AppState, IdentityServices, http::router};
use crate::{
    application::{
        identity::testing::World,
        session::{SessionPolicy, testing::account_with_session},
    },
    features::debate::api::{GetDebate, ModerateComment, PostComment},
    infrastructure::{
        clock::{ManualClock, ts},
        config::Config,
        crypto::new_id,
        db::query_opt,
        face::FaceVerifier,
        flags::{DEBATE, Flags},
        secrets::SecretBox,
    },
};

const ORIGIN: &str = "http://127.0.0.1:3000";

fn app(w: &World) -> axum::Router {
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
        flags: Arc::new(Flags::with(&[DEBATE])),
        clock: Arc::new(ManualClock::new(w.now)),
        face: FaceVerifier::Synthetic,
        identity: Arc::new(IdentityServices {
            secrets: SecretBox::from_key(&[9; 32]),
            address: None,
        }),
    };
    let options = LeptosOptions::builder()
        .output_name("dwc")
        .site_root("target/site")
        .build();
    router(state, options)
}

async fn call(
    app: &axum::Router,
    path: &str,
    body: &str,
    cookie: Option<&str>,
    origin: &str,
) -> (StatusCode, String) {
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
    let bytes = axum::body::to_bytes(res.into_body(), 1 << 20)
        .await
        .unwrap();
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

async fn session(w: &World, code: &str, roles: &[&str]) -> (String, String) {
    let (user, token) = account_with_session(
        &w.t.db,
        w.now,
        SessionPolicy::from_secs(900, 8 * 3600),
        code,
        "active",
        roles,
    )
    .await;
    (user, format!("dwc_session={token}"))
}

async fn proposal(w: &World, author: &str, status: &str) -> String {
    let id = new_id();
    let tx = w.t.db.write().await.unwrap();
    let j: String = query_opt(
        &tx,
        "SELECT id FROM jurisdictions WHERE code = '4202404'",
        (),
    )
    .await
    .unwrap()
    .unwrap()
    .get(0)
    .unwrap();
    let published = (status != "draft").then(|| ts(w.now));
    let hash = published.as_ref().map(|_| "b".repeat(64));
    tx.execute(
        "INSERT INTO proposals (id, kind, jurisdiction_id, author_id, title, summary, body, normalized_title,
           normalized_body_hash, text_hash, status, created_at, updated_at, published_at)
         VALUES (?1, 'normal_law', ?2, ?3, 'Título', 'Resumo', 'Texto', '', '', ?4, ?5, ?6, ?6, ?7)",
        (id.as_str(), j, author, hash, status, ts(w.now), published),
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    id
}

#[tokio::test]
async fn posting_requires_session_same_origin_and_territory() {
    let w = World::new().await;
    let app = app(&w);
    let (author, author_cookie) = session(&w, "4202404", &["Cidadao"]).await;
    let (_, outsider) = session(&w, "4106902", &["Cidadao"]).await;
    let id = proposal(&w, &author, "in_debate").await;
    let form = format!("id={id}&body=Coment%C3%A1rio+de+rede");

    let (status, body) = call(&app, PostComment::PATH, &form, None, ORIGIN).await;
    assert!(!status.is_success());
    assert!(body.contains("Unauthenticated"), "{body}");
    let (status, body) = call(
        &app,
        PostComment::PATH,
        &form,
        Some("dwc_session=forged"),
        ORIGIN,
    )
    .await;
    assert!(
        !status.is_success() && body.contains("Unauthenticated"),
        "{body}"
    );
    let (status, _) = call(
        &app,
        PostComment::PATH,
        &form,
        Some(&author_cookie),
        "https://evil.example",
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "cross-origin rejected");
    let (_, body) = call(&app, PostComment::PATH, &form, Some(&outsider), ORIGIN).await;
    assert!(body.contains("Forbidden"), "other territory: {body}");
    let (status, body) = call(&app, PostComment::PATH, &form, Some(&author_cookie), ORIGIN).await;
    assert!(status.is_success(), "{body}");

    // Public read (no session) shows the comment; a citizen cannot moderate.
    let (status, body) = call(&app, GetDebate::PATH, &format!("id={id}"), None, ORIGIN).await;
    assert!(
        status.is_success() && body.contains("Comentário de rede"),
        "{body}"
    );
    let view: crate::domain::debate::view::DebateView = serde_json::from_str(&body).unwrap();
    let comment = view.comments[0].id.clone();
    let (_, body) = call(
        &app,
        ModerateComment::PATH,
        &format!("comment_id={comment}&decision=Remove&reason=Sem+permiss%C3%A3o"),
        Some(&author_cookie),
        ORIGIN,
    )
    .await;
    assert!(body.contains("Forbidden"), "{body}");

    // Drafts are not readable, even by id.
    let draft = proposal(&w, &author, "draft").await;
    let (status, body) = call(&app, GetDebate::PATH, &format!("id={draft}"), None, ORIGIN).await;
    assert!(!status.is_success() && body.contains("NotFound"), "{body}");
}
