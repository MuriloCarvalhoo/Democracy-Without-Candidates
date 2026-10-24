//! HTTP client for the private Python face engine on loopback (`face-service/`).
//!
//! - Every request carries the shared secret header; loopback alone is not trusted.
//! - The template is the Facenet512 embedding as little-endian `f32` bytes; the
//!   identity use case encrypts it at rest.
//! - Images, embeddings and the key are never logged.
//! - The engine does not validate liveness: responses must state
//!   `"liveness": "not_validated"` and [`FaceServiceClient::liveness_validated`]
//!   is `false`. A match is not proof of a live person.

use std::{path::Path, time::Duration};

use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
use reqwest::{StatusCode, header::HeaderValue};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

use super::face::{FaceOutcome, FaceSample, FaceTemplate};
use crate::error::AppError;

pub const KEY_HEADER: &str = "X-DWC-Face-Key";
/// Same limit as the Python service default (`DWC_FACE_MAX_IMAGE_BYTES`).
pub const MAX_IMAGE_BYTES: usize = 2 * 1024 * 1024;
/// Facenet512 embedding size.
pub const EMBEDDING_DIM: usize = 512;
const MAX_CHALLENGE_LEN: usize = 128;
const MIN_KEY_LEN: usize = 32;
const CONNECT_TIMEOUT: Duration = Duration::from_millis(500);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone)]
pub struct FaceServiceClient {
    pub base_url: String,
    key: HeaderValue,
    http: reqwest::Client,
}

impl std::fmt::Debug for FaceServiceClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FaceServiceClient")
            .field("base_url", &self.base_url)
            .field("key", &"<redacted>")
            .finish()
    }
}

/// Reads the shared secret written for both processes (trimmed, >= 32 chars).
pub fn read_key_file(path: &Path) -> Result<String, String> {
    let key = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read {}: {}", path.display(), e.kind()))?
        .trim()
        .to_owned();
    if key.len() < MIN_KEY_LEN {
        return Err(format!(
            "face service key must have at least {MIN_KEY_LEN} characters"
        ));
    }
    Ok(key)
}

#[derive(Serialize)]
struct EmbedRequest<'a> {
    image_b64: String,
    challenge: &'a str,
}

#[derive(Serialize)]
struct CompareRequest<'a> {
    embedding: Vec<f32>,
    image_b64: String,
    challenge: &'a str,
}

/// Only `not_validated` is accepted: the service cannot claim liveness that Rust
/// has no evidence for.
#[derive(Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Liveness {
    NotValidated,
}

#[derive(Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum EmbedStatus {
    Ok,
    Unusable,
}

#[derive(Deserialize)]
struct EmbedResponse {
    status: EmbedStatus,
    #[serde(default)]
    embedding: Option<Vec<f32>>,
    challenge: String,
    #[allow(dead_code)]
    liveness: Liveness,
}

#[derive(Deserialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
enum CompareStatus {
    Match,
    NoMatch,
    Unusable,
}

#[derive(Deserialize)]
struct CompareResponse {
    status: CompareStatus,
    challenge: String,
    #[allow(dead_code)]
    liveness: Liveness,
}

#[derive(Deserialize)]
struct HealthResponse {
    status: String,
    #[allow(dead_code)]
    liveness: Liveness,
}

fn unavailable() -> AppError {
    AppError::Unavailable("serviço facial indisponível; tente novamente em instantes".into())
}

fn invalid_response() -> AppError {
    AppError::Unavailable("resposta inválida do serviço facial".into())
}

impl FaceServiceClient {
    pub fn new(base_url: &str, key: &str) -> Result<Self, AppError> {
        Self::with_timeout(base_url, key, REQUEST_TIMEOUT)
    }

    fn with_timeout(base_url: &str, key: &str, timeout: Duration) -> Result<Self, AppError> {
        let mut key = HeaderValue::from_str(key)
            .map_err(|_| AppError::Unavailable("chave do serviço facial inválida".into()))?;
        key.set_sensitive(true);
        let http = reqwest::Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(timeout)
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| unavailable())?;
        Ok(Self {
            base_url: base_url.trim_end_matches('/').to_owned(),
            key,
            http,
        })
    }

    /// Always `false`: the engine does not validate liveness server-side.
    pub fn liveness_validated(&self) -> bool {
        false
    }

    pub async fn health(&self) -> Result<(), AppError> {
        let response = self
            .http
            .get(format!("{}/healthz", self.base_url))
            .header(KEY_HEADER, self.key.clone())
            .send()
            .await;
        let health: HealthResponse = Self::read(response, "/healthz").await?;
        if health.status == "ok" {
            Ok(())
        } else {
            Err(invalid_response())
        }
    }

    pub async fn enroll(&self, sample: FaceSample<'_>) -> Result<FaceTemplate, AppError> {
        if sample.image.is_empty() {
            return Err(AppError::validation("face", "imagem vazia"));
        }
        let request = EmbedRequest {
            image_b64: encode_image(sample.image)?,
            challenge: checked_challenge(sample.challenge)?,
        };
        let response: EmbedResponse = self.post("/v1/embed", &request).await?;
        if response.challenge != sample.challenge {
            tracing::warn!("face service echoed a different challenge");
            return Err(invalid_response());
        }
        match (response.status, response.embedding) {
            (EmbedStatus::Ok, Some(embedding)) if valid_embedding(&embedding) => {
                Ok(encode_template(&embedding))
            }
            (EmbedStatus::Unusable, _) => Err(AppError::validation(
                "face",
                "nenhum rosto único utilizável na imagem; tente novamente",
            )),
            _ => {
                tracing::warn!("face service returned a malformed embedding");
                Err(invalid_response())
            }
        }
    }

    pub async fn verify(
        &self,
        template: &FaceTemplate,
        sample: FaceSample<'_>,
    ) -> Result<FaceOutcome, AppError> {
        if sample.image.is_empty() {
            return Ok(FaceOutcome::Unusable);
        }
        let request = CompareRequest {
            embedding: decode_template(template)?,
            image_b64: encode_image(sample.image)?,
            challenge: checked_challenge(sample.challenge)?,
        };
        let response: CompareResponse = self.post("/v1/compare", &request).await?;
        if response.challenge != sample.challenge {
            tracing::warn!("face service echoed a different challenge");
            return Err(invalid_response());
        }
        Ok(match response.status {
            CompareStatus::Match => FaceOutcome::Match,
            CompareStatus::NoMatch => FaceOutcome::NoMatch,
            CompareStatus::Unusable => FaceOutcome::Unusable,
        })
    }

    async fn post<T: DeserializeOwned>(
        &self,
        path: &'static str,
        body: &impl Serialize,
    ) -> Result<T, AppError> {
        let response = self
            .http
            .post(format!("{}{path}", self.base_url))
            .header(KEY_HEADER, self.key.clone())
            .json(body)
            .send()
            .await;
        Self::read(response, path).await
    }

    /// Maps transport and status failures; logs only path, status and error kind.
    async fn read<T: DeserializeOwned>(
        response: Result<reqwest::Response, reqwest::Error>,
        path: &'static str,
    ) -> Result<T, AppError> {
        let response = response.map_err(|e| {
            tracing::warn!(
                path,
                timeout = e.is_timeout(),
                connect = e.is_connect(),
                "face service request failed"
            );
            unavailable()
        })?;
        match response.status() {
            StatusCode::OK => {}
            StatusCode::SERVICE_UNAVAILABLE => {
                return Err(AppError::Unavailable(
                    "serviço facial ocupado; tente novamente em instantes".into(),
                ));
            }
            StatusCode::PAYLOAD_TOO_LARGE => {
                return Err(AppError::validation(
                    "face",
                    "imagem acima do limite de tamanho",
                ));
            }
            status => {
                tracing::warn!(path, %status, "face service rejected request");
                return Err(invalid_response());
            }
        }
        response.json::<T>().await.map_err(|e| {
            tracing::warn!(
                path,
                timeout = e.is_timeout(),
                "face service response unreadable"
            );
            invalid_response()
        })
    }
}

fn encode_image(image: &[u8]) -> Result<String, AppError> {
    if image.len() > MAX_IMAGE_BYTES {
        return Err(AppError::validation(
            "face",
            "imagem acima do limite de 2 MiB",
        ));
    }
    Ok(B64.encode(image))
}

fn checked_challenge(challenge: &str) -> Result<&str, AppError> {
    if challenge.is_empty() || challenge.len() > MAX_CHALLENGE_LEN {
        return Err(AppError::validation(
            "face",
            "desafio de verificação inválido",
        ));
    }
    Ok(challenge)
}

fn valid_embedding(embedding: &[f32]) -> bool {
    embedding.len() == EMBEDDING_DIM && embedding.iter().all(|v| v.is_finite())
}

fn encode_template(embedding: &[f32]) -> FaceTemplate {
    FaceTemplate(embedding.iter().flat_map(|v| v.to_le_bytes()).collect())
}

fn decode_template(template: &FaceTemplate) -> Result<Vec<f32>, AppError> {
    if template.0.len() != EMBEDDING_DIM * 4 {
        tracing::error!("stored face template has an unexpected size");
        return Err(AppError::Internal);
    }
    let embedding: Vec<f32> = template
        .0
        .chunks_exact(4)
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect();
    if !valid_embedding(&embedding) {
        tracing::error!("stored face template is not a finite embedding");
        return Err(AppError::Internal);
    }
    Ok(embedding)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        Json, Router,
        http::{HeaderMap, StatusCode},
        routing::{get, post},
    };
    use serde_json::{Value, json};
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    const KEY: &str = "0123456789abcdef0123456789abcdef-test";

    #[derive(Clone, Default)]
    struct Seen {
        authorized: Arc<AtomicUsize>,
    }

    fn stored() -> Vec<f32> {
        (0..EMBEDDING_DIM)
            .map(|i| i as f32 / 1000.0 - 0.25)
            .collect()
    }

    fn image_of(body: &Value) -> Vec<u8> {
        B64.decode(body["image_b64"].as_str().unwrap()).unwrap()
    }

    fn authorized(headers: &HeaderMap, seen: &Seen) -> bool {
        let ok = headers.get(KEY_HEADER).and_then(|v| v.to_str().ok()) == Some(KEY);
        if ok {
            seen.authorized.fetch_add(1, Ordering::SeqCst);
        }
        ok
    }

    /// Simulates the Python API. The image bytes select the scenario.
    async fn mock() -> (String, Seen) {
        let seen = Seen::default();
        let s1 = seen.clone();
        let s2 = seen.clone();
        let s3 = seen.clone();
        let app = Router::new()
            .route(
                "/healthz",
                get(move |headers: HeaderMap| async move {
                    if !authorized(&headers, &s3) {
                        return (StatusCode::UNAUTHORIZED, Json(json!({"error": "unauthorized"})));
                    }
                    (StatusCode::OK, Json(json!({"status": "ok", "engine": "fake", "liveness": "not_validated"})))
                }),
            )
            .route(
                "/v1/embed",
                post(move |headers: HeaderMap, Json(body): Json<Value>| async move {
                    if !authorized(&headers, &s1) {
                        return (StatusCode::UNAUTHORIZED, Json(json!({"error": "unauthorized"})));
                    }
                    let challenge = body["challenge"].clone();
                    let ok = |embedding: Value, challenge: Value, liveness: &str| {
                        (StatusCode::OK, Json(json!({"status": "ok", "embedding": embedding, "challenge": challenge, "liveness": liveness})))
                    };
                    match image_of(&body).as_slice() {
                        b"face" => ok(json!(stored()), challenge, "not_validated"),
                        b"none" => (StatusCode::OK, Json(json!({"status": "unusable", "reason": "no_face", "challenge": challenge, "liveness": "not_validated"}))),
                        b"short" => ok(json!([0.1, 0.2]), challenge, "not_validated"),
                        b"validated" => ok(json!(stored()), challenge, "validated"),
                        b"other-challenge" => ok(json!(stored()), json!("x"), "not_validated"),
                        b"busy" => (StatusCode::SERVICE_UNAVAILABLE, Json(json!({"error": "busy"}))),
                        b"slow" => {
                            tokio::time::sleep(Duration::from_secs(2)).await;
                            ok(json!(stored()), challenge, "not_validated")
                        }
                        _ => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "engine_error"}))),
                    }
                }),
            )
            .route(
                "/v1/compare",
                post(move |headers: HeaderMap, Json(body): Json<Value>| async move {
                    if !authorized(&headers, &s2) {
                        return (StatusCode::UNAUTHORIZED, Json(json!({"error": "unauthorized"})));
                    }
                    let sent: Vec<f32> = serde_json::from_value(body["embedding"].clone()).unwrap();
                    assert_eq!(sent, stored(), "template must round-trip exactly");
                    let status = match image_of(&body).as_slice() {
                        b"face" => "match",
                        b"other" => "no_match",
                        _ => "unusable",
                    };
                    (StatusCode::OK, Json(json!({"status": status, "distance": 0.1, "threshold": 0.3, "challenge": body["challenge"], "liveness": "not_validated"})))
                }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        (url, seen)
    }

    fn sample<'a>(image: &'a [u8]) -> FaceSample<'a> {
        FaceSample {
            image,
            challenge: "nonce-1",
        }
    }

    async fn client() -> (FaceServiceClient, Seen) {
        let (url, seen) = mock().await;
        let client =
            FaceServiceClient::with_timeout(&format!("{url}/"), KEY, Duration::from_millis(300))
                .unwrap();
        (client, seen)
    }

    fn template() -> FaceTemplate {
        encode_template(&stored())
    }

    #[tokio::test]
    async fn enroll_returns_little_endian_template_with_auth_header() {
        let (c, seen) = client().await;
        let t = c.enroll(sample(b"face")).await.unwrap();
        assert_eq!(t.0.len(), EMBEDDING_DIM * 4);
        assert_eq!(&t.0[..4], &stored()[0].to_le_bytes());
        assert_eq!(decode_template(&t).unwrap(), stored());
        assert_eq!(seen.authorized.load(Ordering::SeqCst), 1);
        c.health().await.unwrap();
        assert!(!c.liveness_validated());
    }

    #[tokio::test]
    async fn verify_maps_match_no_match_unusable() {
        let (c, _) = client().await;
        let t = template();
        assert_eq!(
            c.verify(&t, sample(b"face")).await.unwrap(),
            FaceOutcome::Match
        );
        assert_eq!(
            c.verify(&t, sample(b"other")).await.unwrap(),
            FaceOutcome::NoMatch
        );
        assert_eq!(
            c.verify(&t, sample(b"none")).await.unwrap(),
            FaceOutcome::Unusable
        );
        assert_eq!(
            c.verify(&t, sample(b"")).await.unwrap(),
            FaceOutcome::Unusable
        );
    }

    #[tokio::test]
    async fn enroll_unusable_is_validation_error() {
        let (c, _) = client().await;
        let err = c.enroll(sample(b"none")).await.unwrap_err();
        assert!(matches!(err, AppError::Validation { ref field, .. } if field == "face"));
        assert!(matches!(
            c.enroll(sample(b"")).await,
            Err(AppError::Validation { .. })
        ));
    }

    #[tokio::test]
    async fn protocol_violations_are_unavailable() {
        let (c, _) = client().await;
        for image in [&b"short"[..], b"validated", b"other-challenge", b"boom"] {
            let err = c.enroll(sample(image)).await.unwrap_err();
            assert_eq!(
                err,
                invalid_response(),
                "{}",
                String::from_utf8_lossy(image)
            );
        }
        assert!(matches!(
            c.enroll(sample(b"busy")).await,
            Err(AppError::Unavailable(m)) if m.contains("ocupado")
        ));
    }

    #[tokio::test]
    async fn timeout_and_unreachable_are_unavailable() {
        let (c, _) = client().await;
        assert_eq!(c.enroll(sample(b"slow")).await.unwrap_err(), unavailable());
        let closed = {
            let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            l.local_addr().unwrap()
        };
        let down = FaceServiceClient::new(&format!("http://{closed}"), KEY).unwrap();
        assert_eq!(down.health().await.unwrap_err(), unavailable());
    }

    #[tokio::test]
    async fn wrong_key_is_rejected_by_service() {
        let (url, seen) = mock().await;
        let c = FaceServiceClient::new(&url, &"w".repeat(40)).unwrap();
        assert_eq!(
            c.enroll(sample(b"face")).await.unwrap_err(),
            invalid_response()
        );
        assert_eq!(seen.authorized.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn local_input_checks_happen_before_any_request() {
        let (c, seen) = client().await;
        let big = vec![1u8; MAX_IMAGE_BYTES + 1];
        assert!(matches!(
            c.enroll(sample(&big)).await,
            Err(AppError::Validation { .. })
        ));
        let no_challenge = FaceSample {
            image: b"face",
            challenge: "",
        };
        assert!(matches!(
            c.enroll(no_challenge).await,
            Err(AppError::Validation { .. })
        ));
        let corrupt = FaceTemplate(vec![0; 7]);
        assert_eq!(
            c.verify(&corrupt, sample(b"face")).await.unwrap_err(),
            AppError::Internal
        );
        assert_eq!(seen.authorized.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn debug_redacts_key_and_key_file_is_validated() {
        let c = FaceServiceClient::new("http://127.0.0.1:8001", KEY).unwrap();
        let debug = format!("{c:?}");
        assert!(!debug.contains(KEY) && debug.contains("<redacted>"));

        let dir = tempfile::tempdir().unwrap();
        let good = dir.path().join("face.key");
        std::fs::write(&good, format!("{KEY}\n")).unwrap();
        assert_eq!(read_key_file(&good).unwrap(), KEY);
        let short = dir.path().join("short.key");
        std::fs::write(&short, "short").unwrap();
        assert!(read_key_file(&short).is_err());
        assert!(read_key_file(&dir.path().join("missing")).is_err());
    }

    /// 32x32 single-colour PNGs (no person): black, near-white, blue.
    const PNG_BLACK: &[u8] = &[
        137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 32, 0, 0, 0, 32, 8,
        2, 0, 0, 0, 252, 24, 237, 163, 0, 0, 0, 25, 73, 68, 65, 84, 120, 218, 237, 193, 49, 1, 0,
        0, 0, 194, 160, 245, 79, 237, 97, 13, 160, 0, 0, 0, 110, 12, 32, 0, 1, 33, 50, 205, 236, 0,
        0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
    ];
    const PNG_WHITE: &[u8] = &[
        137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 32, 0, 0, 0, 32, 8,
        2, 0, 0, 0, 252, 24, 237, 163, 0, 0, 0, 41, 73, 68, 65, 84, 120, 218, 237, 205, 49, 1, 0,
        0, 8, 3, 32, 181, 127, 226, 93, 150, 208, 15, 10, 208, 73, 234, 211, 212, 51, 129, 64, 32,
        16, 8, 4, 2, 129, 64, 112, 101, 1, 18, 47, 3, 46, 19, 243, 41, 33, 0, 0, 0, 0, 73, 69, 78,
        68, 174, 66, 96, 130,
    ];
    const PNG_BLUE: &[u8] = &[
        137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 32, 0, 0, 0, 32, 8,
        2, 0, 0, 0, 252, 24, 237, 163, 0, 0, 0, 43, 73, 68, 65, 84, 120, 218, 237, 205, 65, 1, 0,
        64, 4, 0, 48, 46, 141, 136, 34, 138, 117, 37, 248, 109, 5, 150, 213, 19, 151, 94, 28, 19,
        8, 4, 2, 129, 64, 32, 16, 8, 4, 91, 62, 154, 190, 1, 168, 246, 190, 159, 255, 0, 0, 0, 0,
        73, 69, 78, 68, 174, 66, 96, 130,
    ];

    /// Contract check against a running Python service (manual):
    /// `DWC_FACE_IT_URL=http://127.0.0.1:8001 DWC_FACE_IT_KEY_FILE=... DWC_FACE_IT_ENGINE=fake|deepface
    ///  cargo test --features ssr live_service_contract -- --ignored`
    #[tokio::test]
    #[ignore = "requires a running face-service"]
    async fn live_service_contract() {
        let url = std::env::var("DWC_FACE_IT_URL").expect("DWC_FACE_IT_URL");
        let key = read_key_file(Path::new(
            &std::env::var("DWC_FACE_IT_KEY_FILE").expect("DWC_FACE_IT_KEY_FILE"),
        ))
        .unwrap();
        let engine = std::env::var("DWC_FACE_IT_ENGINE").expect("DWC_FACE_IT_ENGINE");
        let c = FaceServiceClient::new(&url, &key).unwrap();
        c.health().await.unwrap();
        assert!(matches!(
            FaceServiceClient::new(&url, &"w".repeat(40))
                .unwrap()
                .health()
                .await,
            Err(AppError::Unavailable(_))
        ));

        // Synthetic face-free images are unusable for both engines.
        assert!(matches!(
            c.enroll(sample(PNG_BLACK)).await,
            Err(AppError::Validation { .. })
        ));
        let t = template();
        assert_eq!(
            c.verify(&t, sample(PNG_BLACK)).await.unwrap(),
            FaceOutcome::Unusable
        );
        assert!(matches!(
            c.enroll(sample(&vec![0xff; MAX_IMAGE_BYTES + 1])).await,
            Err(AppError::Validation { .. })
        ));

        match engine.as_str() {
            "fake" => {
                let t = c.enroll(sample(PNG_WHITE)).await.unwrap();
                assert_eq!(
                    c.verify(&t, sample(PNG_WHITE)).await.unwrap(),
                    FaceOutcome::Match
                );
                assert_eq!(
                    c.verify(&t, sample(PNG_BLUE)).await.unwrap(),
                    FaceOutcome::NoMatch
                );
            }
            "deepface" => {
                for image in [PNG_WHITE, PNG_BLUE] {
                    assert!(matches!(
                        c.enroll(sample(image)).await,
                        Err(AppError::Validation { .. })
                    ));
                }
            }
            other => panic!("unknown DWC_FACE_IT_ENGINE {other}"),
        }
    }
}
