//! Startup configuration. Required values fail explicitly; nothing is invented.

use std::{
    env,
    net::{IpAddr, SocketAddr},
    path::PathBuf,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Environment {
    Development,
    Test,
    Production,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdentityProviderKind {
    /// Deterministic fake face verification. Development/test only.
    Synthetic,
    /// Private Python face engine reachable on loopback.
    FaceService,
}

#[derive(Debug, Clone)]
pub struct CountryConfig {
    pub code: &'static str,
    pub name: &'static str,
    pub locale: &'static str,
    pub timezone: &'static str,
    pub catalog_path: &'static str,
}

pub fn country(code: &str) -> Option<CountryConfig> {
    match code {
        "BR" => Some(CountryConfig {
            code: "BR",
            name: "Brasil",
            locale: "pt-BR",
            timezone: "America/Sao_Paulo",
            catalog_path: "catalog/br/jurisdictions.csv",
        }),
        _ => None,
    }
}

#[derive(Debug, Clone)]
pub struct Config {
    pub env: Environment,
    pub db_path: PathBuf,
    pub country: CountryConfig,
    pub public_origin: String,
    pub identity_provider: IdentityProviderKind,
    pub face_service_url: Option<String>,
    /// Shared secret sent to the face service; required with `face-service`.
    pub face_service_key_file: Option<PathBuf>,
    pub secret_key_file: Option<PathBuf>,
    /// Ed25519 key for audit checkpoints. Required in production; dev/test default
    /// to `audit-signing.key` next to the database (generated on first use).
    pub audit_signing_key_file: PathBuf,
    pub session_idle_secs: i64,
    pub session_absolute_secs: i64,
    pub secure_cookies: bool,
    /// BrasilAPI base URL. Overridable (`DWC_BRASILAPI_URL`) only outside production,
    /// so tests can point at a local mock.
    pub brasilapi_url: String,
    /// Interval of the deadline/vote-closing and scheduled-notification tickers.
    /// Overridable (`DWC_TICKER_SECS`) only outside production (E2E/demos).
    pub ticker_secs: i64,
    /// Reverse proxies whose `X-Forwarded-For` is trusted for the per-IP limit
    /// (`DWC_TRUSTED_PROXY`, comma-separated; empty: socket peer only).
    pub trusted_proxies: Vec<IpAddr>,
    /// `DWC_DEV_SKIP_TOTP=1`: TOTP codes are not checked (development/test only;
    /// rejected in production). Enrollment still stores a secret.
    pub dev_skip_totp: bool,
    /// `DWC_DEBUG=1`: debug tracing of instrumented functions (development/test
    /// only; rejected in production). Read by `server::observability` at startup.
    pub debug: bool,
    /// Requests handled at once (`DWC_MAX_IN_FLIGHT`, default 1024); beyond it
    /// the server answers 503 at once instead of queueing without bound.
    pub max_in_flight: usize,
}

/// Agent proposal pending load calibration (`just load` ramp to saturation).
pub const DEFAULT_MAX_IN_FLIGHT: usize = 1024;

pub const BRASILAPI_URL: &str = "https://brasilapi.com.br";

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("missing required setting {0}")]
    Missing(&'static str),
    #[error("invalid setting {0}: {1}")]
    Invalid(&'static str, String),
}

impl Config {
    pub fn from_env(site_addr: SocketAddr) -> Result<Self, ConfigError> {
        Self::from_lookup(site_addr, |key| env::var(key).ok())
    }

    pub fn from_lookup(
        site_addr: SocketAddr,
        get: impl Fn(&str) -> Option<String>,
    ) -> Result<Self, ConfigError> {
        let required = |key: &'static str| {
            get(key)
                .filter(|v| !v.trim().is_empty())
                .ok_or(ConfigError::Missing(key))
        };

        let env = match required("DWC_ENV")?.as_str() {
            "development" => Environment::Development,
            "test" => Environment::Test,
            "production" => Environment::Production,
            other => return Err(ConfigError::Invalid("DWC_ENV", other.into())),
        };
        let db_path = PathBuf::from(required("DWC_DB_PATH")?);
        let country_code = required("DWC_COUNTRY")?;
        let country =
            country(&country_code).ok_or(ConfigError::Invalid("DWC_COUNTRY", country_code))?;

        let production = env == Environment::Production;
        let public_origin = match get("DWC_PUBLIC_ORIGIN") {
            Some(origin) => origin.trim_end_matches('/').to_owned(),
            None if production => return Err(ConfigError::Missing("DWC_PUBLIC_ORIGIN")),
            None => format!("http://{site_addr}"),
        };
        if production && !public_origin.starts_with("https://") {
            return Err(ConfigError::Invalid(
                "DWC_PUBLIC_ORIGIN",
                "production requires https".into(),
            ));
        }
        if !production && !site_addr.ip().is_loopback() {
            return Err(ConfigError::Invalid(
                "LEPTOS_SITE_ADDR",
                "development/test must bind to loopback".into(),
            ));
        }

        let identity_provider = match get("DWC_IDENTITY_PROVIDER").as_deref() {
            Some("synthetic") => IdentityProviderKind::Synthetic,
            Some("face-service") => IdentityProviderKind::FaceService,
            Some(other) => return Err(ConfigError::Invalid("DWC_IDENTITY_PROVIDER", other.into())),
            None => return Err(ConfigError::Missing("DWC_IDENTITY_PROVIDER")),
        };
        if production && identity_provider == IdentityProviderKind::Synthetic {
            return Err(ConfigError::Invalid(
                "DWC_IDENTITY_PROVIDER",
                "synthetic identity provider is rejected in production".into(),
            ));
        }
        let face_service_url = get("DWC_FACE_SERVICE_URL");
        let face_service_key_file = get("DWC_FACE_SERVICE_KEY_FILE")
            .filter(|v| !v.trim().is_empty())
            .map(PathBuf::from);
        if identity_provider == IdentityProviderKind::FaceService {
            let url = face_service_url
                .as_deref()
                .ok_or(ConfigError::Missing("DWC_FACE_SERVICE_URL"))?;
            if !(url.starts_with("http://127.0.0.1:") || url.starts_with("http://[::1]:")) {
                return Err(ConfigError::Invalid(
                    "DWC_FACE_SERVICE_URL",
                    "face service must be loopback".into(),
                ));
            }
            if face_service_key_file.is_none() {
                return Err(ConfigError::Missing("DWC_FACE_SERVICE_KEY_FILE"));
            }
        }

        let secret_key_file = get("DWC_SECRET_KEY_FILE").map(PathBuf::from);
        if production && secret_key_file.is_none() {
            return Err(ConfigError::Missing("DWC_SECRET_KEY_FILE"));
        }

        let audit_signing_key_file = match get("DWC_AUDIT_SIGNING_KEY_FILE") {
            Some(path) if !path.trim().is_empty() => PathBuf::from(path),
            _ if production => return Err(ConfigError::Missing("DWC_AUDIT_SIGNING_KEY_FILE")),
            _ => db_path.with_file_name("audit-signing.key"),
        };

        let brasilapi_url = match get("DWC_BRASILAPI_URL") {
            None => BRASILAPI_URL.to_owned(),
            Some(_) if production => {
                return Err(ConfigError::Invalid(
                    "DWC_BRASILAPI_URL",
                    "fixed in production".into(),
                ));
            }
            Some(url) => url.trim_end_matches('/').to_owned(),
        };

        let trusted_proxies = get("DWC_TRUSTED_PROXY")
            .unwrap_or_default()
            .split(|c: char| c == ',' || c.is_whitespace())
            .filter(|v| !v.is_empty())
            .map(|v| {
                v.parse::<IpAddr>()
                    .map(|ip| ip.to_canonical())
                    .map_err(|_| ConfigError::Invalid("DWC_TRUSTED_PROXY", v.into()))
            })
            .collect::<Result<Vec<_>, _>>()?;

        // Development/test switches: refused in production.
        let dev_flag = |key: &'static str, why: &str| -> Result<bool, ConfigError> {
            match get(key).as_deref().map(str::trim) {
                None | Some("") | Some("0") | Some("false") => Ok(false),
                Some("1") | Some("true") if production => {
                    Err(ConfigError::Invalid(key, why.into()))
                }
                Some("1") | Some("true") => Ok(true),
                Some(other) => Err(ConfigError::Invalid(key, other.into())),
            }
        };
        let dev_skip_totp = dev_flag("DWC_DEV_SKIP_TOTP", "TOTP cannot be skipped in production")?;
        let debug = dev_flag("DWC_DEBUG", "debug tracing is not allowed in production")?;

        let parse_secs = |key: &'static str, default: i64| -> Result<i64, ConfigError> {
            match get(key) {
                None => Ok(default),
                Some(_) if production => {
                    Err(ConfigError::Invalid(key, "fixed in production".into()))
                }
                Some(v) => v
                    .parse::<i64>()
                    .ok()
                    .filter(|s| *s > 0)
                    .ok_or(ConfigError::Invalid(key, v)),
            }
        };

        let max_in_flight = match get("DWC_MAX_IN_FLIGHT") {
            None => DEFAULT_MAX_IN_FLIGHT,
            Some(v) => v
                .trim()
                .parse::<usize>()
                .ok()
                .filter(|n| *n > 0)
                .ok_or(ConfigError::Invalid("DWC_MAX_IN_FLIGHT", v))?,
        };

        Ok(Self {
            env,
            db_path,
            country,
            public_origin,
            identity_provider,
            face_service_url,
            face_service_key_file,
            secret_key_file,
            audit_signing_key_file,
            session_idle_secs: parse_secs("DWC_SESSION_IDLE_SECS", 15 * 60)?,
            session_absolute_secs: parse_secs("DWC_SESSION_ABSOLUTE_SECS", 8 * 60 * 60)?,
            secure_cookies: production,
            brasilapi_url,
            ticker_secs: parse_secs("DWC_TICKER_SECS", 30)?,
            trusted_proxies,
            dev_skip_totp,
            debug,
            max_in_flight,
        })
    }

    pub fn is_production(&self) -> bool {
        self.env == Environment::Production
    }

    pub fn session_cookie_name(&self) -> &'static str {
        if self.secure_cookies {
            "__Host-dwc_session"
        } else {
            "dwc_session"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn addr() -> SocketAddr {
        "127.0.0.1:3000".parse().unwrap()
    }

    fn cfg(pairs: &[(&str, &str)]) -> Result<Config, ConfigError> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        Config::from_lookup(addr(), |k| map.get(k).cloned())
    }

    const DEV: &[(&str, &str)] = &[
        ("DWC_ENV", "development"),
        ("DWC_DB_PATH", "data/dev.db"),
        ("DWC_COUNTRY", "BR"),
        ("DWC_IDENTITY_PROVIDER", "synthetic"),
    ];

    #[test]
    fn development_defaults() {
        let c = cfg(DEV).unwrap();
        assert_eq!(c.session_idle_secs, 900);
        assert_eq!(c.session_cookie_name(), "dwc_session");
        assert_eq!(c.public_origin, "http://127.0.0.1:3000");
        assert_eq!(
            c.audit_signing_key_file,
            PathBuf::from("data/audit-signing.key")
        );
    }

    #[test]
    fn production_requires_audit_signing_key() {
        let base = [
            ("DWC_ENV", "production"),
            ("DWC_DB_PATH", "/var/lib/dwc/br.db"),
            ("DWC_COUNTRY", "BR"),
            ("DWC_PUBLIC_ORIGIN", "https://br.example.invalid"),
            ("DWC_SECRET_KEY_FILE", "/etc/dwc/key"),
            ("DWC_IDENTITY_PROVIDER", "face-service"),
            ("DWC_FACE_SERVICE_URL", "http://127.0.0.1:8001"),
            ("DWC_FACE_SERVICE_KEY_FILE", "/etc/dwc/face.key"),
        ];
        assert!(matches!(
            cfg(&base),
            Err(ConfigError::Missing("DWC_AUDIT_SIGNING_KEY_FILE"))
        ));
        let mut with_key = base.to_vec();
        with_key.push(("DWC_AUDIT_SIGNING_KEY_FILE", "/etc/dwc/audit.key"));
        assert_eq!(
            cfg(&with_key).unwrap().audit_signing_key_file,
            PathBuf::from("/etc/dwc/audit.key")
        );
    }

    #[test]
    fn missing_required_fails() {
        assert!(matches!(
            cfg(&DEV[1..]),
            Err(ConfigError::Missing("DWC_ENV"))
        ));
        assert!(matches!(
            cfg(&[("DWC_ENV", "development")]),
            Err(ConfigError::Missing("DWC_DB_PATH"))
        ));
    }

    #[test]
    fn production_rejects_synthetic_identity() {
        let err = cfg(&[
            ("DWC_ENV", "production"),
            ("DWC_DB_PATH", "/var/lib/dwc/br.db"),
            ("DWC_COUNTRY", "BR"),
            ("DWC_PUBLIC_ORIGIN", "https://br.example.invalid"),
            ("DWC_SECRET_KEY_FILE", "/etc/dwc/key"),
            ("DWC_IDENTITY_PROVIDER", "synthetic"),
        ])
        .unwrap_err();
        assert!(matches!(
            err,
            ConfigError::Invalid("DWC_IDENTITY_PROVIDER", _)
        ));
    }

    #[test]
    fn production_uses_host_cookie_and_fixed_timeouts() {
        let base = [
            ("DWC_ENV", "production"),
            ("DWC_DB_PATH", "/var/lib/dwc/br.db"),
            ("DWC_COUNTRY", "BR"),
            ("DWC_PUBLIC_ORIGIN", "https://br.example.invalid"),
            ("DWC_SECRET_KEY_FILE", "/etc/dwc/key"),
            ("DWC_IDENTITY_PROVIDER", "face-service"),
            ("DWC_FACE_SERVICE_URL", "http://127.0.0.1:8001"),
            ("DWC_FACE_SERVICE_KEY_FILE", "/etc/dwc/face.key"),
            ("DWC_AUDIT_SIGNING_KEY_FILE", "/etc/dwc/audit.key"),
        ];
        let c = cfg(&base).unwrap();
        assert_eq!(c.session_cookie_name(), "__Host-dwc_session");
        let mut with_override = base.to_vec();
        with_override.push(("DWC_SESSION_IDLE_SECS", "5"));
        assert!(cfg(&with_override).is_err());
        let mut mock_cep = base.to_vec();
        mock_cep.push(("DWC_BRASILAPI_URL", "http://127.0.0.1:9"));
        assert!(
            cfg(&mock_cep).is_err(),
            "address mock rejected in production"
        );
        assert_eq!(c.brasilapi_url, BRASILAPI_URL);
        assert_eq!(c.ticker_secs, 30);
        let mut fast_ticker = base.to_vec();
        fast_ticker.push(("DWC_TICKER_SECS", "1"));
        assert!(cfg(&fast_ticker).is_err(), "ticker fixed in production");
    }

    #[test]
    fn max_in_flight_defaults_and_rejects_zero() {
        assert_eq!(cfg(DEV).unwrap().max_in_flight, DEFAULT_MAX_IN_FLIGHT);
        let mut pairs = DEV.to_vec();
        pairs.push(("DWC_MAX_IN_FLIGHT", "64"));
        assert_eq!(cfg(&pairs).unwrap().max_in_flight, 64);
        pairs.pop();
        pairs.push(("DWC_MAX_IN_FLIGHT", "0"));
        assert!(cfg(&pairs).is_err());
    }

    #[test]
    fn face_service_must_be_loopback() {
        let mut pairs = DEV.to_vec();
        pairs[3] = ("DWC_IDENTITY_PROVIDER", "face-service");
        pairs.push(("DWC_FACE_SERVICE_KEY_FILE", "/etc/dwc/face.key"));
        pairs.push(("DWC_FACE_SERVICE_URL", "http://10.0.0.5:8001"));
        assert!(matches!(
            cfg(&pairs),
            Err(ConfigError::Invalid("DWC_FACE_SERVICE_URL", _))
        ));
    }

    #[test]
    fn face_service_requires_key_file() {
        let mut pairs = DEV.to_vec();
        pairs[3] = ("DWC_IDENTITY_PROVIDER", "face-service");
        pairs.push(("DWC_FACE_SERVICE_URL", "http://127.0.0.1:8001"));
        assert!(matches!(
            cfg(&pairs),
            Err(ConfigError::Missing("DWC_FACE_SERVICE_KEY_FILE"))
        ));
        pairs.push(("DWC_FACE_SERVICE_KEY_FILE", "  "));
        assert!(matches!(
            cfg(&pairs),
            Err(ConfigError::Missing("DWC_FACE_SERVICE_KEY_FILE"))
        ));
        pairs.pop();
        pairs.push(("DWC_FACE_SERVICE_KEY_FILE", "/etc/dwc/face.key"));
        let c = cfg(&pairs).unwrap();
        assert_eq!(
            c.face_service_key_file.as_deref(),
            Some(std::path::Path::new("/etc/dwc/face.key"))
        );
    }

    #[test]
    fn skip_totp_is_dev_only() {
        assert!(!cfg(DEV).unwrap().dev_skip_totp);
        let mut pairs = DEV.to_vec();
        pairs.push(("DWC_DEV_SKIP_TOTP", "1"));
        assert!(cfg(&pairs).unwrap().dev_skip_totp);
        pairs.pop();
        pairs.push(("DWC_DEV_SKIP_TOTP", "yes"));
        assert!(matches!(
            cfg(&pairs),
            Err(ConfigError::Invalid("DWC_DEV_SKIP_TOTP", _))
        ));
        let prod = [
            ("DWC_ENV", "production"),
            ("DWC_DB_PATH", "/var/lib/dwc/br.db"),
            ("DWC_COUNTRY", "BR"),
            ("DWC_PUBLIC_ORIGIN", "https://br.example.invalid"),
            ("DWC_SECRET_KEY_FILE", "/etc/dwc/key"),
            ("DWC_IDENTITY_PROVIDER", "face-service"),
            ("DWC_FACE_SERVICE_URL", "http://127.0.0.1:8001"),
            ("DWC_FACE_SERVICE_KEY_FILE", "/etc/dwc/face.key"),
            ("DWC_AUDIT_SIGNING_KEY_FILE", "/etc/dwc/audit.key"),
            ("DWC_DEV_SKIP_TOTP", "1"),
        ];
        assert!(matches!(
            cfg(&prod),
            Err(ConfigError::Invalid("DWC_DEV_SKIP_TOTP", _))
        ));
    }

    #[test]
    fn debug_is_dev_only() {
        assert!(!cfg(DEV).unwrap().debug);
        let mut pairs = DEV.to_vec();
        pairs.push(("DWC_DEBUG", "1"));
        assert!(cfg(&pairs).unwrap().debug);
        pairs.pop();
        pairs.push(("DWC_DEBUG", "verbose"));
        assert!(matches!(
            cfg(&pairs),
            Err(ConfigError::Invalid("DWC_DEBUG", _))
        ));
        let prod = [
            ("DWC_ENV", "production"),
            ("DWC_DB_PATH", "/var/lib/dwc/br.db"),
            ("DWC_COUNTRY", "BR"),
            ("DWC_PUBLIC_ORIGIN", "https://br.example.invalid"),
            ("DWC_SECRET_KEY_FILE", "/etc/dwc/key"),
            ("DWC_IDENTITY_PROVIDER", "face-service"),
            ("DWC_FACE_SERVICE_URL", "http://127.0.0.1:8001"),
            ("DWC_FACE_SERVICE_KEY_FILE", "/etc/dwc/face.key"),
            ("DWC_AUDIT_SIGNING_KEY_FILE", "/etc/dwc/audit.key"),
            ("DWC_DEBUG", "1"),
        ];
        assert!(matches!(
            cfg(&prod),
            Err(ConfigError::Invalid("DWC_DEBUG", _))
        ));
    }

    #[test]
    fn synthetic_does_not_require_face_key() {
        assert!(cfg(DEV).unwrap().face_service_key_file.is_none());
    }

    #[test]
    fn trusted_proxy_list_is_optional_and_validated() {
        assert!(cfg(DEV).unwrap().trusted_proxies.is_empty());
        let mut pairs = DEV.to_vec();
        pairs.push(("DWC_TRUSTED_PROXY", "127.0.0.1, ::1"));
        assert_eq!(
            cfg(&pairs).unwrap().trusted_proxies,
            vec![
                "127.0.0.1".parse::<IpAddr>().unwrap(),
                "::1".parse().unwrap()
            ]
        );
        pairs.pop();
        pairs.push(("DWC_TRUSTED_PROXY", "localhost"));
        assert!(matches!(
            cfg(&pairs),
            Err(ConfigError::Invalid("DWC_TRUSTED_PROXY", _))
        ));
    }

    #[test]
    fn unknown_country_fails() {
        let mut pairs = DEV.to_vec();
        pairs[2] = ("DWC_COUNTRY", "XX");
        assert!(matches!(
            cfg(&pairs),
            Err(ConfigError::Invalid("DWC_COUNTRY", _))
        ));
    }
}
