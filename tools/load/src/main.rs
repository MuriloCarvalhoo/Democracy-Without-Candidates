//! Goose load test for one isolated instance. Normally started by `run.sh`, which
//! owns the app process, the disposable database and the invariant checks.
//!
//! Goose options are passed through (`--host`, `--users`, `--run-time`,
//! `--report-file`...). `DWC_LOAD_SUMMARY` names the JSON summary to write.
//! Exit code 2 when thresholds are violated (experimental limits, not capacity).

mod scenarios;
mod serverfn;

use std::{collections::BTreeMap, sync::atomic::Ordering};

use goose::{metrics::GooseMetrics, prelude::*};
use serde::Serialize;

/// Working interpretation of RNF02 for this harness (see docs/development/sdd-plan.md).
const READ_P95_MS: usize = 2_000;
const WRITE_P95_MS: usize = 3_000;
const MAX_FAILURE_RATE: f64 = 0.01;
const WRITES: &[&str] = &["increment_counter", "voting/cast"];

#[derive(Serialize)]
struct RequestSummary {
    requests: usize,
    failures: usize,
    p50_ms: usize,
    p95_ms: usize,
    p99_ms: usize,
    max_ms: usize,
}

#[derive(Serialize)]
struct Summary {
    users: usize,
    duration_secs: usize,
    total_requests: usize,
    total_failures: usize,
    failure_rate: f64,
    throughput_rps: f64,
    increments_confirmed: u64,
    votes_confirmed: u64,
    voters_available: usize,
    requests: BTreeMap<String, RequestSummary>,
    violations: Vec<String>,
}

/// Percentile from Goose's (rounded) response-time histogram.
fn percentile(times: &BTreeMap<usize, usize>, total: usize, p: f64) -> usize {
    let rank = ((total as f64) * p).ceil().max(1.0) as usize;
    let mut seen = 0;
    for (ms, count) in times {
        seen += count;
        if seen >= rank {
            return *ms;
        }
    }
    0
}

fn summarize(metrics: &GooseMetrics) -> Summary {
    let mut requests = BTreeMap::new();
    let mut violations = Vec::new();
    let (mut total, mut failures) = (0, 0);
    for agg in metrics.requests.values() {
        let n = agg.success_count + agg.fail_count;
        let t = &agg.raw_data;
        let s = RequestSummary {
            requests: n,
            failures: agg.fail_count,
            p50_ms: percentile(&t.times, t.counter, 0.50),
            p95_ms: percentile(&t.times, t.counter, 0.95),
            p99_ms: percentile(&t.times, t.counter, 0.99),
            max_ms: t.maximum_time,
        };
        let limit = if WRITES.contains(&agg.path.as_str()) {
            WRITE_P95_MS
        } else {
            READ_P95_MS
        };
        if s.p95_ms >= limit {
            violations.push(format!("{}: p95 {} ms >= {limit} ms", agg.path, s.p95_ms));
        }
        total += n;
        failures += agg.fail_count;
        requests.insert(agg.path.clone(), s);
    }
    let failure_rate = if total == 0 {
        1.0
    } else {
        failures as f64 / total as f64
    };
    if total == 0 {
        violations.push("no requests completed".into());
    }
    if failure_rate > MAX_FAILURE_RATE {
        violations.push(format!("failure rate {:.3}% > 1%", failure_rate * 100.0));
    }
    let duration_secs = metrics.duration.max(1);
    Summary {
        users: metrics.maximum_users,
        duration_secs: metrics.duration,
        total_requests: total,
        total_failures: failures,
        failure_rate,
        throughput_rps: total as f64 / duration_secs as f64,
        increments_confirmed: scenarios::INCREMENTS_CONFIRMED.load(Ordering::Relaxed),
        votes_confirmed: scenarios::VOTES_CONFIRMED.load(Ordering::Relaxed),
        voters_available: scenarios::voters().map_or(0, |v| v.tokens.len()),
        requests,
        violations,
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let metrics = GooseAttack::initialize()?
        .register_scenario(scenarios::democracy()?)
        .execute()
        .await?;
    let summary = summarize(&metrics);
    let json = serde_json::to_string_pretty(&summary)?;
    if let Ok(path) = std::env::var("DWC_LOAD_SUMMARY") {
        std::fs::write(path, &json)?;
    }
    println!("{json}");
    if !summary.violations.is_empty() {
        eprintln!("thresholds violated: {:?}", summary.violations);
        std::process::exit(2);
    }
    Ok(())
}
