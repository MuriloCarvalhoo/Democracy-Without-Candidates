//! Audit/transparency data shared by server and browser. No I/O.

use serde::{Deserialize, Serialize};

/// Kind of the event appended for every restricted (auditor) read.
pub const RESTRICTED_ACCESS: &str = "audit.restricted_access";

/// Events per page in audit listings.
pub const PAGE_SIZE: u32 = 50;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventView {
    pub seq: i64,
    pub occurred_at: String,
    pub kind: String,
    pub restricted: bool,
    /// Only present in the restricted (auditor) view.
    pub actor_ref: Option<String>,
    pub subject: Option<String>,
    pub payload: String,
    pub hash: String,
}

/// Result of recomputing the local hash chain (all events, any visibility).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChainSummary {
    Valid { events: i64, head: String },
    Broken { at_seq: i64 },
}

/// Signed statement "the chain of `country` had `head_hash` at `seq`".
/// Exported outside the VPS; only an external copy detects later rewrites.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Checkpoint {
    pub country: String,
    pub seq: i64,
    pub head_hash: String,
    pub created_at: String,
    /// Hex ed25519 verifying key.
    pub public_key: String,
    /// Hex ed25519 signature over [`Checkpoint::message`].
    pub signature: String,
}

impl Checkpoint {
    pub fn message(country: &str, seq: i64, head_hash: &str, created_at: &str) -> String {
        format!("dwc-audit-checkpoint:v1\n{country}\n{seq}\n{head_hash}\n{created_at}")
    }

    pub fn signed_message(&self) -> String {
        Self::message(&self.country, self.seq, &self.head_hash, &self.created_at)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditListing {
    pub events: Vec<EventView>,
    /// `before` cursor for the next (older) page, if any.
    pub next_before: Option<i64>,
    pub kinds: Vec<String>,
    pub chain: ChainSummary,
    pub checkpoints: Vec<Checkpoint>,
    pub public_key: Option<String>,
}

/// Event kinds accepted as filters: lowercase ASCII, digits, `.`, `_`, `-`.
pub fn valid_kind(kind: &str) -> bool {
    !kind.is_empty()
        && kind.len() <= 64
        && kind
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"._-".contains(&b))
}

/// Payload keys that must never reach the audit log (secrets, biometrics,
/// contact data, network identifiers, ballots). Checked per `_`-separated word.
const FORBIDDEN_WORDS: &[&str] = &[
    "password",
    "secret",
    "token",
    "totp",
    "face",
    "biometric",
    "biometrics",
    "embedding",
    "email",
    "ip",
    "cpf",
    "ballot",
];

pub fn forbidden_payload_key(key: &str) -> bool {
    key.to_ascii_lowercase()
        .split(['_', '-', '.'])
        .any(|w| FORBIDDEN_WORDS.contains(&w))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_filter_and_payload_denylist() {
        assert!(valid_kind("proposal.created"));
        assert!(!valid_kind("Proposal"));
        assert!(!valid_kind("a b"));
        assert!(!valid_kind(""));
        for bad in [
            "password_hash",
            "email",
            "client_ip",
            "face_template",
            "ballot",
        ] {
            assert!(forbidden_payload_key(bad), "{bad}");
        }
        for ok in [
            "recipient",
            "description",
            "surface",
            "scope",
            "proposal_id",
        ] {
            assert!(!forbidden_payload_key(ok), "{ok}");
        }
    }

    #[test]
    fn checkpoint_message_binds_all_fields() {
        let a = Checkpoint::message("BR", 1, "h", "t");
        assert_ne!(a, Checkpoint::message("BR", 2, "h", "t"));
        assert_ne!(a, Checkpoint::message("PT", 1, "h", "t"));
        assert_ne!(a, Checkpoint::message("BR", 1, "h2", "t"));
        assert_ne!(a, Checkpoint::message("BR", 1, "h", "t2"));
    }
}
