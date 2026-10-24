//! Minimal message catalog. pt-BR is the initial language; each feature keeps its
//! own key file so new languages can be added without touching components.

mod pt_br;

/// Returns the message for `key`, or the key itself when missing (visible in review).
pub fn t(key: &'static str) -> &'static str {
    pt_br::lookup(key).unwrap_or(key)
}
