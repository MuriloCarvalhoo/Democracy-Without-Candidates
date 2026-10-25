//! Use cases: authorization, transactions and coordination. Server-only.

pub mod actor;
pub mod admin;
pub mod audit;
pub mod checkpoint;
pub mod debate;
pub mod idempotency;
pub mod identity;
pub mod notify;
pub mod phases;
pub mod plebiscite;
pub mod proposals;
pub mod reauth;
pub mod session;
pub mod setup;
pub mod signatures;
pub mod statistics;
pub mod voting;

#[cfg(test)]
mod debate_tests;
#[cfg(test)]
mod legislative_tests;
#[cfg(test)]
mod voting_tests;
