//! Ed25519 key for audit checkpoints. The key file (hex seed) lives outside Git and
//! outside the database. Development/test may generate it (mode 0600); production
//! requires an existing file that is not readable by group/others.

use std::{fs, io::Write, path::Path};

use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use rand::RngCore;

#[derive(Debug, thiserror::Error)]
pub enum KeyError {
    #[error("audit signing key {0} not found")]
    Missing(String),
    #[error("audit signing key {0}: {1}")]
    Invalid(String, String),
    #[error("audit signing key {0}: {1}")]
    Io(String, std::io::Error),
}

/// Loads the key, generating it when absent and `allow_create` (non-production).
pub fn load_or_create(path: &Path, allow_create: bool) -> Result<SigningKey, KeyError> {
    let name = path.display().to_string();
    match fs::read_to_string(path) {
        Ok(text) => {
            check_permissions(path, !allow_create)?;
            parse(&text).ok_or_else(|| KeyError::Invalid(name, "expected 64 hex chars".into()))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound && allow_create => {
            let mut seed = [0u8; 32];
            rand::rng().fill_bytes(&mut seed);
            write_private(path, &format!("{}\n", hex::encode(seed)))
                .map_err(|e| KeyError::Io(name.clone(), e))?;
            tracing::warn!(path = %name, "generated development audit signing key; production requires DWC_AUDIT_SIGNING_KEY_FILE");
            Ok(SigningKey::from_bytes(&seed))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(KeyError::Missing(name)),
        Err(e) => Err(KeyError::Io(name, e)),
    }
}

fn parse(text: &str) -> Option<SigningKey> {
    let bytes: [u8; 32] = hex::decode(text.trim()).ok()?.try_into().ok()?;
    Some(SigningKey::from_bytes(&bytes))
}

#[cfg(unix)]
fn write_private(path: &Path, content: &str) -> std::io::Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(content.as_bytes())
}

#[cfg(not(unix))]
fn write_private(path: &Path, content: &str) -> std::io::Result<()> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    file.write_all(content.as_bytes())
}

#[cfg(unix)]
fn check_permissions(path: &Path, strict: bool) -> Result<(), KeyError> {
    use std::os::unix::fs::PermissionsExt;
    let name = path.display().to_string();
    let mode = fs::metadata(path)
        .map_err(|e| KeyError::Io(name.clone(), e))?
        .permissions()
        .mode();
    if mode & 0o077 != 0 {
        if strict {
            return Err(KeyError::Invalid(
                name,
                "must not be readable by group/others (chmod 600)".into(),
            ));
        }
        tracing::warn!(path = %name, "audit signing key is readable by group/others");
    }
    Ok(())
}

#[cfg(not(unix))]
fn check_permissions(_: &Path, _: bool) -> Result<(), KeyError> {
    Ok(())
}

pub fn public_key_hex(key: &SigningKey) -> String {
    hex::encode(key.verifying_key().to_bytes())
}

pub fn sign_hex(key: &SigningKey, message: &str) -> String {
    hex::encode(key.sign(message.as_bytes()).to_bytes())
}

/// True only for a well-formed key and a valid signature over `message`.
pub fn verify_hex(public_key: &str, message: &str, signature: &str) -> bool {
    let Some(key) = hex::decode(public_key)
        .ok()
        .and_then(|b| <[u8; 32]>::try_from(b).ok())
        .and_then(|b| VerifyingKey::from_bytes(&b).ok())
    else {
        return false;
    };
    let Some(sig) = hex::decode(signature)
        .ok()
        .and_then(|b| <[u8; 64]>::try_from(b).ok())
        .map(|b| Signature::from_bytes(&b))
    else {
        return false;
    };
    key.verify_strict(message.as_bytes(), &sig).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_private_key_once_and_signs() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("keys/audit.key");
        assert!(matches!(
            load_or_create(&path, false),
            Err(KeyError::Missing(_))
        ));
        let key = load_or_create(&path, true).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        let again = load_or_create(&path, false).unwrap();
        assert_eq!(public_key_hex(&key), public_key_hex(&again));

        let pk = public_key_hex(&key);
        let sig = sign_hex(&key, "m");
        assert!(verify_hex(&pk, "m", &sig));
        assert!(!verify_hex(&pk, "m2", &sig));
        assert!(!verify_hex(&pk, "m", "00"));
        assert!(!verify_hex("zz", "m", &sig));
    }

    #[cfg(unix)]
    #[test]
    fn production_rejects_permissive_key_file() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("audit.key");
        fs::write(&path, format!("{}\n", "11".repeat(32))).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(matches!(
            load_or_create(&path, false),
            Err(KeyError::Invalid(..))
        ));
        assert!(load_or_create(&path, true).is_ok(), "dev only warns");
        fs::write(&path, "not-hex").unwrap();
        assert!(load_or_create(&path, true).is_err());
    }
}
