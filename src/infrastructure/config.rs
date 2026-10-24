//! Startup configuration. Required values fail explicitly; nothing is invented.

use std::{env, net::SocketAddr, path::PathBuf};

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
}

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
    fn synthetic_does_not_require_face_key() {
        assert!(cfg(DEV).unwrap().face_service_key_file.is_none());
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
