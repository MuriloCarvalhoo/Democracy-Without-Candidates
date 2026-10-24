//! pt-BR catalog split by feature to keep parallel work conflict-free.

mod admin;
mod common;

pub fn lookup(key: &str) -> Option<&'static str> {
    common::lookup(key).or_else(|| admin::lookup(key))
}
