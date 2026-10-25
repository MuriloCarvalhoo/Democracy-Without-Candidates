//! HTTP status of errors (`just e2e`, no browser): server functions and SSR pages
//! answer 401/403/404/422 for the matching `AppError`, never 500, and keep the
//! JSON error body the client decodes.

mod support;

use dwc::infrastructure::db::Db;
use support::{App, account_with_session, http_get_with, http_post, seed_dev};

const NO_ID: &str = "00000000-0000-0000-0000-000000000000";

#[tokio::test]
#[ignore = "requires built app; run with `just e2e`"]
async fn errors_map_to_http_status_for_server_fns_and_pages() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("e2e.db");
    seed_dev(&db_path);
    let (citizen, admin) = {
        let db = Db::open(&db_path).await.unwrap();
        let (_, citizen) = account_with_session(&db, "cidada@example.test", "pol-cidadao").await;
        let (_, admin) =
            account_with_session(&db, "admin@example.test", "pol-administrator-access").await;
        (citizen, admin)
    };
    let app = App::start(&db_path, &[]);
    let origin = app.url("");
    let cookie = |token: &str| format!("dwc_session={token}");
    let (citizen, admin) = (cookie(&citizen), cookie(&admin));
    let post = |cookie: &str, path: &str, body: &str| {
        let mut headers = vec![
            ("Origin", origin.as_str()),
            ("Content-Type", "application/x-www-form-urlencoded"),
        ];
        headers.extend((!cookie.is_empty()).then_some(("Cookie", cookie)));
        http_post(app.port, path, &headers, body)
    };

    // Server functions: status follows the error, body stays the JSON AppError.
    let (status, body) = post("", "/api/iam/policies", "");
    assert_eq!(status, 401, "{body}");
    assert!(body.contains("Unauthenticated"), "{body}");
    let (status, body) = post(&citizen, "/api/iam/policies", "");
    assert_eq!(status, 403, "{body}");
    assert!(body.contains("Forbidden"), "{body}");
    let (status, body) = post(&admin, "/api/iam/policy", &format!("id={NO_ID}"));
    assert_eq!(status, 404, "{body}");
    assert!(body.contains("NotFound"), "{body}");
    let draft = r#"{"id":null,"input":{"kind":"NormalLaw","level":"Municipal","title":"x","summary":"Resumo válido aqui","body":"Corpo válido com mais de vinte caracteres","proposed_setup":null}}"#;
    let (status, body) = http_post(
        app.port,
        "/api/proposals/save_draft",
        &[
            ("Origin", origin.as_str()),
            ("Content-Type", "application/json"),
            ("Cookie", citizen.as_str()),
        ],
        draft,
    );
    assert_eq!(status, 422, "{body}");
    assert!(body.contains("Validation"), "{body}");
    let (status, body) = post(&admin, "/api/iam/policies", "");
    assert_eq!(status, 200, "{body}");

    // SSR pages: the main resource error sets the document status.
    let page = |path: &str, cookie: &str| {
        let headers: Vec<(&str, &str)> = (!cookie.is_empty())
            .then_some(("Cookie", cookie))
            .into_iter()
            .collect();
        http_get_with(app.port, path, &headers)
    };
    for (path, who, expected) in [
        (format!("/propostas/{NO_ID}"), "", 404),
        ("/admin".to_owned(), "", 401),
        ("/admin".to_owned(), citizen.as_str(), 403),
        ("/admin".to_owned(), admin.as_str(), 200),
        ("/admin/iam/politicas".to_owned(), citizen.as_str(), 403),
        (format!("/admin/iam/politicas/{NO_ID}"), admin.as_str(), 404),
        ("/propostas".to_owned(), "", 200),
        ("/rota-inexistente".to_owned(), "", 404),
    ] {
        let (status, body) = page(&path, who);
        assert_eq!(status, expected, "GET {path} as {who:?}");
        assert!(body.contains("</html>"), "GET {path}: full document");
    }
    // Declared icon: browsers stop probing the missing /favicon.ico.
    assert!(page("/propostas", "").1.contains(r#"rel="icon""#));
}

/// Credential forms must POST when the WebAssembly app is not running (slow
/// network, blocked script, crash): a native GET would put the e-mail and the
/// password in the URL, the browser history and proxy logs.
#[tokio::test]
#[ignore = "requires built app; run with `just e2e`"]
async fn credential_forms_never_fall_back_to_get() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("e2e.db");
    seed_dev(&db_path);
    let citizen = {
        let db = Db::open(&db_path).await.unwrap();
        account_with_session(&db, "cidada@example.test", "pol-cidadao")
            .await
            .1
    };
    let app = App::start(&db_path, &[]);
    let cookie = format!("dwc_session={citizen}");
    for (path, who) in [
        ("/login", ""),
        ("/cadastro", ""),
        ("/recuperar-senha", ""),
        ("/alterar-senha", cookie.as_str()),
        ("/perfil/codigos", cookie.as_str()),
    ] {
        let headers: Vec<(&str, &str)> = (!who.is_empty())
            .then_some(("Cookie", who))
            .into_iter()
            .collect();
        let (status, body) = http_get_with(app.port, path, &headers);
        assert_eq!(status, 200, "GET {path}");
        let forms: Vec<&str> = body
            .split("<form")
            .skip(1)
            .map(|f| f.split('>').next().unwrap_or(""))
            .collect();
        assert!(!forms.is_empty(), "GET {path}: no form rendered");
        for form in forms {
            assert!(
                form.contains(r#"method="post""#),
                "GET {path}: <form{form}> may submit as GET"
            );
        }
    }
}
