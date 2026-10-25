//! pt-BR catalog split by feature to keep parallel work conflict-free.

mod admin;
mod audit;
mod auth;
mod common;
mod debate;
mod notifications;
mod profile;
mod proposals;
mod voting;

pub fn lookup(key: &str) -> Option<&'static str> {
    common::lookup(key)
        .or_else(|| audit::lookup(key))
        .or_else(|| notifications::lookup(key))
        .or_else(|| auth::lookup(key))
        .or_else(|| profile::lookup(key))
        .or_else(|| proposals::lookup(key))
        .or_else(|| admin::lookup(key))
        .or_else(|| debate::lookup(key))
        .or_else(|| voting::lookup(key))
}
