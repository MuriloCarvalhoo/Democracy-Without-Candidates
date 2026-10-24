//! pt-BR catalog split by feature to keep parallel work conflict-free.

mod auth;
mod common;
mod profile;

pub fn lookup(key: &str) -> Option<&'static str> {
    common::lookup(key)
        .or_else(|| auth::lookup(key))
        .or_else(|| profile::lookup(key))
}
