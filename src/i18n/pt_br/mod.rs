//! pt-BR catalog split by feature to keep parallel work conflict-free.

mod common;
mod proposals;

pub fn lookup(key: &str) -> Option<&'static str> {
    common::lookup(key).or_else(|| proposals::lookup(key))
}
