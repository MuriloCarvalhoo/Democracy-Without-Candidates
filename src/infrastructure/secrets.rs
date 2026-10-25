//! Encryption at rest for face templates and TOTP secrets (XChaCha20-Poly1305).
//! The key lives outside the database: `DWC_SECRET_KEY_FILE` (required in
//! production) or, in development/test only, a generated key next to the database.
//! Each ciphertext is bound to its purpose and owner through associated data.

use std::path::{Path, PathBuf};

use chacha20poly1305::{
    XChaCha20Poly1305, XNonce,
    aead::{Aead, Generate, KeyInit, Payload},
};

use super::config::Config;
use crate::error::AppError;

const VERSION: u8 = 1;
const NONCE_LEN: usize = 24;

pub struct SecretBox {
    cipher: XChaCha20Poly1305,
}

impl SecretBox {
    pub fn from_key(key: &[u8; 32]) -> Self {
        Self {
            cipher: XChaCha20Poly1305::new_from_slice(key).expect("32-byte key"),
        }
    }

    /// Loads the configured key file, or a development key under the database directory.
    pub fn load(config: &Config) -> Result<Self, String> {
        match &config.secret_key_file {
            Some(path) => read_key(path, config.is_production()).map(|k| Self::from_key(&k)),
            None if config.is_production() => Err("DWC_SECRET_KEY_FILE is required".into()),
            None => {
                let path = dev_key_path(&config.db_path);
                if !path.exists() {
                    write_dev_key(&path)?;
                    tracing::warn!(path = %path.display(), "generated development secret key; never use it in production");
                }
                read_key(&path, false).map(|k| Self::from_key(&k))
            }
        }
    }

    /// `version || nonce || ciphertext`; `aad` binds the value to its row and purpose.
    pub fn seal(&self, plaintext: &[u8], aad: &str) -> Result<Vec<u8>, AppError> {
        let nonce = XNonce::generate();
        let ct = self
            .cipher
            .encrypt(
                &nonce,
                Payload {
                    msg: plaintext,
                    aad: aad.as_bytes(),
                },
            )
            .map_err(AppError::internal)?;
        let mut out = Vec::with_capacity(1 + NONCE_LEN + ct.len());
        out.push(VERSION);
        out.extend_from_slice(&nonce);
        out.extend_from_slice(&ct);
        Ok(out)
    }

    pub fn open(&self, sealed: &[u8], aad: &str) -> Result<Vec<u8>, AppError> {
        if sealed.len() < 1 + NONCE_LEN || sealed[0] != VERSION {
            return Err(AppError::internal("sealed secret has an unknown format"));
        }
        let nonce = XNonce::try_from(&sealed[1..1 + NONCE_LEN]).map_err(AppError::internal)?;
        self.cipher
            .decrypt(
                &nonce,
                Payload {
                    msg: &sealed[1 + NONCE_LEN..],
                    aad: aad.as_bytes(),
                },
            )
            .map_err(|_| {
                tracing::error!("secret decryption failed (wrong key or tampered value)");
                AppError::Internal
            })
    }
}

fn dev_key_path(db_path: &Path) -> PathBuf {
    let name = db_path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "dwc".into());
    db_path.with_file_name(format!("{name}.dev-secret.key"))
}

/// Accepts 32 raw bytes or 64 hex characters. `strict` (production) requires an
/// owner-only file (no group/other permissions); otherwise it only warns.
fn read_key(path: &Path, strict: bool) -> Result<[u8; 32], String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(path)
            .map_err(|e| format!("secret key {}: {e}", path.display()))?
            .permissions()
            .mode();
        if mode & 0o077 != 0 {
            if strict {
                return Err(format!(
                    "secret key {}: must not be accessible by group/others (chmod 600)",
                    path.display()
                ));
            }
            tracing::warn!(path = %path.display(), "secret key is accessible by group/others");
        }
    }
    let raw = std::fs::read(path).map_err(|e| format!("secret key {}: {e}", path.display()))?;
    let bytes = match raw.len() {
        32 => raw,
        _ => hex::decode(String::from_utf8_lossy(&raw).trim())
            .map_err(|_| format!("secret key {}: expected 32 bytes or 64 hex", path.display()))?,
    };
    bytes
        .try_into()
        .map_err(|_| format!("secret key {}: expected 32 bytes", path.display()))
}

fn write_dev_key(path: &Path) -> Result<(), String> {
    use std::io::Write;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut file = options
        .open(path)
        .map_err(|e| format!("secret key {}: {e}", path.display()))?;
    file.write_all(super::crypto::random_token(32).as_bytes())
        .map_err(|e| format!("secret key {}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seal_open_binds_aad_and_detects_tampering() {
        let b = SecretBox::from_key(&[7; 32]);
        let sealed = b.seal(b"secret", "totp:u1").unwrap();
        assert_eq!(b.open(&sealed, "totp:u1").unwrap(), b"secret");
        assert!(b.open(&sealed, "totp:u2").is_err(), "other owner");
        let mut tampered = sealed.clone();
        *tampered.last_mut().unwrap() ^= 1;
        assert!(b.open(&tampered, "totp:u1").is_err());
        assert!(
            SecretBox::from_key(&[8; 32])
                .open(&sealed, "totp:u1")
                .is_err()
        );
        assert_ne!(
            b.seal(b"secret", "totp:u1").unwrap(),
            sealed,
            "random nonce"
        );
    }

    #[test]
    fn dev_key_is_generated_once_with_restricted_permissions() {
        let dir = tempfile::tempdir().unwrap();
        let path = dev_key_path(&dir.path().join("dev.db"));
        write_dev_key(&path).unwrap();
        let key = read_key(&path, true).unwrap();
        assert!(write_dev_key(&path).is_err(), "never overwritten");
        assert_eq!(read_key(&path, true).unwrap(), key);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
    }

    #[cfg(unix)]
    #[test]
    fn production_key_must_be_owner_only_and_32_bytes() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let write = |name: &str, content: &[u8], mode: u32| {
            let path = dir.path().join(name);
            std::fs::write(&path, content).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode)).unwrap();
            path
        };
        let hex_key = format!("{}\n", "ab".repeat(32));
        let open = write("open.key", hex_key.as_bytes(), 0o640);
        assert!(read_key(&open, true).unwrap_err().contains("chmod 600"));
        assert_eq!(
            read_key(&open, false).unwrap(),
            [0xab; 32],
            "dev only warns"
        );
        let private = write("private.key", hex_key.as_bytes(), 0o600);
        assert_eq!(read_key(&private, true).unwrap(), [0xab; 32]);
        assert_eq!(
            read_key(&write("raw.key", &[7; 32], 0o400), true).unwrap(),
            [7; 32]
        );
        for (name, content) in [
            ("short.key", "ab".repeat(20)),
            ("long.key", "ab".repeat(33)),
        ] {
            assert!(read_key(&write(name, content.as_bytes(), 0o600), true).is_err());
        }
    }
}
