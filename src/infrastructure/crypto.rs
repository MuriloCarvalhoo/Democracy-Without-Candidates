use rand::RngCore;
use sha2::{Digest, Sha256};

/// Random opaque token encoded as lowercase hex (`bytes` of entropy).
pub fn random_token(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    rand::rng().fill_bytes(&mut buf);
    hex::encode(buf)
}

pub fn sha256_hex(input: impl AsRef<[u8]>) -> String {
    hex::encode(Sha256::digest(input.as_ref()))
}

pub fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}
