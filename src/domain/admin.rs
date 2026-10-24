//! Instance administration views shared by server and browser. No personal data
//! beyond what the Admin País already manages (auditor accounts).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlagView {
    pub key: String,
    pub description: String,
    /// State loaded at startup and used by the running process.
    pub effective: bool,
    /// State stored in the database; `None` when never written (disabled).
    pub stored: Option<bool>,
    pub implemented: bool,
    pub pending_restart: bool,
}

/// A stored change only applies after restart; blocked features never apply.
pub fn pending_restart(effective: bool, stored: Option<bool>, implemented: bool) -> bool {
    implemented && stored.unwrap_or(false) != effective
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstanceStats {
    /// `(status, count)` pairs; aggregate only.
    pub users_by_status: Vec<(String, i64)>,
    pub db_bytes: u64,
    pub migrations: Vec<String>,
    /// `(catalog, checksum prefix, imported_at)`.
    pub catalogs: Vec<(String, String, String)>,
    pub effective_flags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdminOverview {
    pub flags: Vec<FlagView>,
    pub stats: InstanceStats,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditorView {
    pub email: String,
    pub name: String,
    pub granted_at: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_restart_only_when_stored_differs() {
        assert!(!pending_restart(false, None, true));
        assert!(!pending_restart(true, Some(true), true));
        assert!(pending_restart(false, Some(true), true));
        assert!(pending_restart(true, Some(false), true));
        assert!(
            pending_restart(true, None, true),
            "row removed: off after restart"
        );
        assert!(
            !pending_restart(false, Some(true), false),
            "blocked feature never applies"
        );
    }
}
