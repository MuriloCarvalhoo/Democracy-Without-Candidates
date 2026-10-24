//! Scenarios. Each user loops over its scenario; weights give the request mix.
//! Add later scenarios (list proposals, sign, vote) here with synthetic identities
//! and register them in `main.rs`, keeping reads and writes separately named so
//! thresholds and invariants can be checked per request.

use std::sync::atomic::{AtomicU64, Ordering};

use goose::prelude::*;

use crate::serverfn::{call, get_expect};

/// Increments the server confirmed with a valid value (compared with the database).
pub static INCREMENTS_CONFIRMED: AtomicU64 = AtomicU64::new(0);

/// Foundation mix per iteration: 19 reads (health 3, home 8, counter 8) and
/// 1 write = 95% reads / 5% writes.
pub fn foundation() -> Result<Scenario, GooseError> {
    Ok(scenario!("foundation")
        .register_transaction(transaction!(health).set_weight(3)?)
        .register_transaction(transaction!(home).set_weight(8)?)
        .register_transaction(transaction!(read_counter).set_weight(8)?)
        .register_transaction(transaction!(increment_counter).set_weight(1)?))
}

async fn health(user: &mut GooseUser) -> TransactionResult {
    get_expect(user, "health", "/healthz", "\"ok\"").await
}

async fn home(user: &mut GooseUser) -> TransactionResult {
    get_expect(user, "home", "/", "data-testid=\"counter-increment\"").await
}

async fn read_counter(user: &mut GooseUser) -> TransactionResult {
    call(user, "get_counter", "").await.map(|_| ())
}

async fn increment_counter(user: &mut GooseUser) -> TransactionResult {
    if let Some(value) = call(user, "increment_counter", "").await?
        && value.as_i64().is_some_and(|v| v > 0)
    {
        INCREMENTS_CONFIRMED.fetch_add(1, Ordering::Relaxed);
    }
    Ok(())
}
