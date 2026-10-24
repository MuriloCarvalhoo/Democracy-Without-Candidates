//! Address providers selected by instance flags. Brazil: BrasilAPI CEP v1, whose
//! field names are preserved in persistence. Results are cached by CEP on the server
//! so registration persists exactly what the server fetched, never browser data.

use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

use serde::Deserialize;
use serde_json::Value;

use super::{config::Config, flags};
use crate::{error::AppError, i18n::t};

const CACHE_TTL: Duration = Duration::from_secs(15 * 60);
const CACHE_MAX: usize = 10_000;

/// BrasilAPI `GET /api/cep/v1/{cep}` response, validated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CepAddress {
    pub cep: String,
    pub state: String,
    pub city: String,
    pub neighborhood: String,
    pub street: String,
    pub service: String,
    /// Serialized `ibge` object (`{"city": ..., "state": ...}`), keys preserved.
    pub ibge: Value,
}

impl CepAddress {
    pub fn ibge_city(&self) -> Option<String> {
        ibge_code(self.ibge.get("city"))
    }
}

fn ibge_code(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::String(s) if !s.trim().is_empty() => Some(s.trim().to_owned()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LookupError {
    InvalidCep,
    NotFound,
    Unavailable,
    InvalidResponse,
}

impl From<LookupError> for AppError {
    fn from(e: LookupError) -> Self {
        match e {
            LookupError::InvalidCep => AppError::validation("cep", t("auth.cep.invalid")),
            LookupError::NotFound => AppError::validation("cep", t("auth.cep.not_found")),
            LookupError::Unavailable => AppError::Unavailable(t("auth.cep.unavailable").into()),
            LookupError::InvalidResponse => {
                AppError::Unavailable(t("auth.cep.invalid_response").into())
            }
        }
    }
}

/// Eight digits, accepting the usual `00000-000` mask.
pub fn normalize_cep(raw: &str) -> Option<String> {
    let digits: String = raw
        .trim()
        .chars()
        .filter(|c| *c != '-' && *c != '.')
        .collect();
    (digits.len() == 8 && digits.chars().all(|c| c.is_ascii_digit())).then_some(digits)
}

/// Provider flags and the country each one serves.
const PROVIDERS: &[(&str, &str)] = &[(flags::ADDRESS_BRASILAPI, "BR")];

pub struct AddressProvider {
    client: reqwest::Client,
    base_url: String,
    cache: Mutex<HashMap<String, (Instant, CepAddress)>>,
}

impl AddressProvider {
    /// Exactly one enabled provider compatible with the instance country, else none.
    pub fn select(flags: &flags::Flags, config: &Config) -> Option<Self> {
        let enabled: Vec<&str> = PROVIDERS
            .iter()
            .filter(|(key, country)| *country == config.country.code && flags.is_enabled(key))
            .map(|(key, _)| *key)
            .collect();
        match enabled.as_slice() {
            [key] if *key == flags::ADDRESS_BRASILAPI => Some(Self::brasilapi(
                &config.brasilapi_url,
                Duration::from_secs(5),
            )),
            _ => None,
        }
    }

    pub fn brasilapi(base_url: &str, timeout: Duration) -> Self {
        let client = reqwest::Client::builder()
            .timeout(timeout)
            .connect_timeout(timeout)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .expect("http client");
        Self {
            client,
            base_url: base_url.trim_end_matches('/').to_owned(),
            cache: Mutex::new(HashMap::new()),
        }
    }

    /// Server-side lookup, served from the cache when fresh.
    pub async fn lookup(&self, raw_cep: &str) -> Result<CepAddress, LookupError> {
        let cep = normalize_cep(raw_cep).ok_or(LookupError::InvalidCep)?;
        if let Some(hit) = self.cached(&cep) {
            return Ok(hit);
        }
        let address = self.fetch(&cep).await?;
        let mut cache = self.cache.lock().expect("cache lock");
        if cache.len() >= CACHE_MAX {
            cache.retain(|_, (at, _)| at.elapsed() < CACHE_TTL);
            if cache.len() >= CACHE_MAX {
                cache.clear();
            }
        }
        cache.insert(cep, (Instant::now(), address.clone()));
        Ok(address)
    }

    fn cached(&self, cep: &str) -> Option<CepAddress> {
        let cache = self.cache.lock().expect("cache lock");
        cache
            .get(cep)
            .filter(|(at, _)| at.elapsed() < CACHE_TTL)
            .map(|(_, a)| a.clone())
    }

    async fn fetch(&self, cep: &str) -> Result<CepAddress, LookupError> {
        let url = format!("{}/api/cep/v1/{cep}", self.base_url);
        let response = self.client.get(url).send().await.map_err(|e| {
            tracing::warn!(timeout = e.is_timeout(), "address provider request failed");
            LookupError::Unavailable
        })?;
        match response.status().as_u16() {
            200 => {}
            404 => return Err(LookupError::NotFound),
            400 => return Err(LookupError::InvalidCep),
            429 | 500..=599 => return Err(LookupError::Unavailable),
            status => {
                tracing::warn!(status, "unexpected address provider status");
                return Err(LookupError::InvalidResponse);
            }
        }
        let body = response
            .bytes()
            .await
            .map_err(|_| LookupError::Unavailable)?;
        parse(cep, &body)
    }
}

#[derive(Deserialize)]
struct Raw {
    cep: String,
    state: String,
    city: String,
    #[serde(default)]
    neighborhood: Option<String>,
    #[serde(default)]
    street: Option<String>,
    #[serde(default)]
    service: Option<String>,
    #[serde(default)]
    ibge: Option<Value>,
}

fn parse(requested: &str, body: &[u8]) -> Result<CepAddress, LookupError> {
    let raw: Raw = serde_json::from_slice(body).map_err(|_| LookupError::InvalidResponse)?;
    let state = raw.state.trim().to_owned();
    let city = raw.city.trim().to_owned();
    let valid = normalize_cep(&raw.cep).as_deref() == Some(requested)
        && state.len() == 2
        && state.chars().all(|c| c.is_ascii_uppercase())
        && !city.is_empty()
        && city.chars().count() <= 100;
    if !valid {
        return Err(LookupError::InvalidResponse);
    }
    let ibge = match raw.ibge {
        Some(Value::Object(map)) => {
            let city = ibge_code(map.get("city")).map_or(Value::Null, Value::String);
            let state = ibge_code(map.get("state")).map_or(Value::Null, Value::String);
            serde_json::json!({ "city": city, "state": state })
        }
        Some(Value::Null) | None => serde_json::json!({ "city": null, "state": null }),
        Some(_) => return Err(LookupError::InvalidResponse),
    };
    let text = |v: Option<String>| v.unwrap_or_default().trim().chars().take(200).collect();
    Ok(CepAddress {
        cep: requested.to_owned(),
        state,
        city,
        neighborhood: text(raw.neighborhood),
        street: text(raw.street),
        service: text(raw.service),
        ibge,
    })
}

/// Local BrasilAPI stand-in for tests (never the network).
#[cfg(test)]
pub mod testing {
    use axum::{Router, extract::Path, http::StatusCode, response::IntoResponse, routing::get};

    pub async fn mock_brasilapi() -> String {
        async fn cep(Path(cep): Path<String>) -> axum::response::Response {
            let ok = |body: serde_json::Value| (StatusCode::OK, axum::Json(body)).into_response();
            match cep.as_str() {
                "89010025" => ok(
                    serde_json::json!({"cep": "89010025", "state": "SC", "city": "Blumenau", "neighborhood": "Centro", "street": "Rua Doutor Luiz de Freitas Melro", "service": "open-cep", "ibge": {"city": "4202404", "state": "42"}}),
                ),
                "88010000" => ok(
                    serde_json::json!({"cep": "88010000", "state": "SC", "city": "Florianópolis", "neighborhood": "Centro", "street": "Rua Felipe Schmidt", "service": "viacep"}),
                ),
                "89000000" => ok(
                    serde_json::json!({"cep": "89000000", "state": "SC", "city": "Blumenau", "neighborhood": "", "street": "", "service": "correios", "ibge": {"city": null, "state": null}}),
                ),
                "33333333" => ok(
                    serde_json::json!({"cep": "33333333", "state": "SC", "city": "Cidade Inexistente", "neighborhood": "", "street": "", "service": "open-cep"}),
                ),
                "44444444" => ok(
                    serde_json::json!({"cep": "44444444", "state": "PR", "city": "Blumenau", "neighborhood": "", "street": "", "service": "open-cep", "ibge": {"city": "4202404", "state": "41"}}),
                ),
                "22222222" => ok(
                    serde_json::json!({"cep": "89010025", "state": "SC", "city": "Blumenau", "service": "open-cep"}),
                ),
                "11111111" => (StatusCode::OK, "<html>").into_response(),
                "55555555" => StatusCode::SERVICE_UNAVAILABLE.into_response(),
                "99999999" => {
                    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                    StatusCode::OK.into_response()
                }
                _ => StatusCode::NOT_FOUND.into_response(),
            }
        }
        let app = Router::new().route("/api/cep/v1/{cep}", get(cep));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await });
        format!("http://{addr}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cep_normalization() {
        assert_eq!(normalize_cep("89010-025").as_deref(), Some("89010025"));
        assert_eq!(normalize_cep(" 01001000 ").as_deref(), Some("01001000"));
        for bad in ["", "8901002", "890100250", "8901002a", "89 010025"] {
            assert_eq!(normalize_cep(bad), None, "{bad}");
        }
    }

    #[tokio::test]
    async fn distinct_errors_and_preserved_fields() {
        let url = testing::mock_brasilapi().await;
        let p = AddressProvider::brasilapi(&url, Duration::from_millis(500));
        let a = p.lookup("89010-025").await.unwrap();
        assert_eq!(a.service, "open-cep");
        assert_eq!(a.ibge_city().as_deref(), Some("4202404"));
        assert_eq!(a.ibge.to_string(), r#"{"city":"4202404","state":"42"}"#);
        let generic = p.lookup("89000000").await.unwrap();
        assert_eq!(
            (generic.street.as_str(), generic.neighborhood.as_str()),
            ("", "")
        );
        assert_eq!(generic.ibge_city(), None);
        assert_eq!(p.lookup("123").await, Err(LookupError::InvalidCep));
        assert_eq!(p.lookup("00000000").await, Err(LookupError::NotFound));
        assert_eq!(
            p.lookup("11111111").await,
            Err(LookupError::InvalidResponse)
        );
        assert_eq!(
            p.lookup("22222222").await,
            Err(LookupError::InvalidResponse),
            "response for another CEP"
        );
        assert_eq!(p.lookup("55555555").await, Err(LookupError::Unavailable));
        assert_eq!(
            p.lookup("99999999").await,
            Err(LookupError::Unavailable),
            "timeout"
        );
    }

    #[tokio::test]
    async fn selection_requires_exactly_one_compatible_provider() {
        let config = Config::from_lookup("127.0.0.1:3000".parse().unwrap(), |k| {
            match k {
                "DWC_ENV" => Some("test"),
                "DWC_DB_PATH" => Some("x.db"),
                "DWC_COUNTRY" => Some("BR"),
                "DWC_IDENTITY_PROVIDER" => Some("synthetic"),
                _ => None,
            }
            .map(String::from)
        })
        .unwrap();
        assert!(AddressProvider::select(&flags::Flags::with(&[]), &config).is_none());
        assert!(
            AddressProvider::select(&flags::Flags::with(&[flags::ADDRESS_BRASILAPI]), &config)
                .is_some()
        );
    }
}
