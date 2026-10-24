//! pt-BR catalog split by feature to keep parallel work conflict-free.

mod common;

pub fn lookup(key: &str) -> Option<&'static str> {
    common::lookup(key)
}
