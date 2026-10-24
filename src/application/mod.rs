//! Use cases: authorization, transactions and coordination. Server-only.

pub mod actor;
pub mod audit;
pub mod checkpoint;
pub mod idempotency;
pub mod identity;
pub mod notify;
pub mod phases;
pub mod proposals;
pub mod reauth;
pub mod session;
pub mod setup;
pub mod signatures;

#[cfg(test)]
mod legislative_tests;
