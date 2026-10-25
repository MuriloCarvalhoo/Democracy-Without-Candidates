//! Scenarios. Each user loops over its scenario; weights give the request mix.
//! Reads and writes are separately named so thresholds and invariants can be
//! checked per request.

use std::sync::{
    OnceLock,
    atomic::{AtomicU64, AtomicUsize, Ordering},
};

use goose::prelude::*;
use serde::Deserialize;

use crate::serverfn::{call, call_as, get_expect};

/// Increments the server confirmed with a valid value (compared with the database).
pub static INCREMENTS_CONFIRMED: AtomicU64 = AtomicU64::new(0);
/// Ballots the server confirmed (`voting/cast` returned a confirmation).
pub static VOTES_CONFIRMED: AtomicU64 = AtomicU64::new(0);
static NEXT_VOTER: AtomicUsize = AtomicUsize::new(0);

/// Synthetic voters written by `dwc seed-load` (path in `DWC_LOAD_VOTERS`).
#[derive(Deserialize)]
pub struct Voters {
    pub proposal_id: String,
    pub cookie_name: String,
    pub tokens: Vec<String>,
}

pub fn voters() -> Option<&'static Voters> {
    static VOTERS: OnceLock<Option<Voters>> = OnceLock::new();
    VOTERS
        .get_or_init(|| {
            let path = std::env::var("DWC_LOAD_VOTERS").ok()?;
            let text = std::fs::read_to_string(path).ok()?;
            serde_json::from_str(&text).ok()
        })
        .as_ref()
}

/// Concentrated vote + mixed reads. On start every user casts exactly one public
/// ballot with its own synthetic voter (session cookie + reauth grant prepared
/// by `seed-load`); then it loops over 19 reads (health 2, home 3, proposals page
/// 4, proposals server fn 5, result 4, counter 1) and 1 write (counter) = 95/5.
pub fn democracy() -> Result<Scenario, GooseError> {
    Ok(scenario!("democracy")
        .register_transaction(transaction!(cast_vote).set_on_start())
        .register_transaction(transaction!(health).set_weight(2)?)
        .register_transaction(transaction!(home).set_weight(3)?)
        .register_transaction(transaction!(proposals_page).set_weight(4)?)
        .register_transaction(transaction!(list_proposals).set_weight(5)?)
        .register_transaction(transaction!(vote_result).set_weight(4)?)
        .register_transaction(transaction!(read_counter).set_weight(1)?)
        .register_transaction(transaction!(increment_counter).set_weight(1)?))
}

async fn cast_vote(user: &mut GooseUser) -> TransactionResult {
    let Some(v) = voters() else {
        return Ok(());
    };
    let i = NEXT_VOTER.fetch_add(1, Ordering::Relaxed);
    let Some(token) = v.tokens.get(i) else {
        return Ok(());
    };
    let cookie = format!("{}={token}", v.cookie_name);
    let choice = ["yes", "yes", "no", "abstain"][i % 4];
    let body = format!(
        "id={}&choice={choice}&public_consent=true&idempotency_key=load-{i:08}",
        v.proposal_id
    );
    if let Some(value) = call_as(user, "voting/cast", &body, Some(&cookie)).await?
        && value.get("proposal_id").is_some()
    {
        VOTES_CONFIRMED.fetch_add(1, Ordering::Relaxed);
    }
    Ok(())
}

async fn health(user: &mut GooseUser) -> TransactionResult {
    get_expect(user, "health", "/healthz", "\"ok\"").await
}

async fn home(user: &mut GooseUser) -> TransactionResult {
    get_expect(user, "home", "/", "data-testid=\"counter-increment\"").await
}

async fn proposals_page(user: &mut GooseUser) -> TransactionResult {
    get_expect(user, "propostas (SSR)", "/propostas", "[Carga]").await
}

async fn list_proposals(user: &mut GooseUser) -> TransactionResult {
    call(user, "proposals/list", "").await.map(|_| ())
}

async fn vote_result(user: &mut GooseUser) -> TransactionResult {
    let id = voters().map(|v| v.proposal_id.as_str()).unwrap_or("none");
    call(user, "voting/result", &format!("id={id}"))
        .await
        .map(|_| ())
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
