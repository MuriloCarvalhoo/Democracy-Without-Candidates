//! pt-BR catalog split by feature to keep parallel work conflict-free.

mod audit;
mod common;
mod notifications;

pub fn lookup(key: &str) -> Option<&'static str> {
    common::lookup(key)
        .or_else(|| audit::lookup(key))
        .or_else(|| notifications::lookup(key))
}
