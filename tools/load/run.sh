#!/usr/bin/env bash
# Load experiment against an isolated release instance (synthetic data only).
#   tools/load/run.sh <users> <duration>     e.g. tools/load/run.sh 10 30s
# Requires `cargo leptos build --release` (just load does it). Starts target/release/dwc
# on a free loopback port with a disposable database, runs Goose, stops the app,
# verifies database invariants and writes everything to artifacts/load/<timestamp>/.
# Exits non-zero on startup timeout, threshold violations (>1% failures, p95 limits)
# or broken invariants. Results are experiments, not capacity claims.
set -euo pipefail

USERS=${1:?usage: run.sh <users> <duration>}
DURATION=${2:?usage: run.sh <users> <duration>}
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
BIN="$ROOT/target/release/dwc"
[[ -x "$BIN" && -f "$ROOT/target/release/hash.txt" ]] || { echo "missing release build: run 'cargo leptos build --release'" >&2; exit 1; }
for tool in curl jq; do command -v "$tool" >/dev/null || { echo "missing: $tool" >&2; exit 1; }; done

OUT="$ROOT/artifacts/load/$(date -u +%Y%m%dT%H%M%SZ)"
mkdir -p "$OUT"
WORK=$(mktemp -d)
APP_PID=""
cleanup() {
  if [[ -n "$APP_PID" ]] && kill -0 "$APP_PID" 2>/dev/null; then
    kill "$APP_PID" 2>/dev/null || true
    wait "$APP_PID" 2>/dev/null || true
  fi
  rm -rf "$WORK"
}
trap cleanup EXIT

PORT=$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1])')
HOST="http://127.0.0.1:$PORT"
app_env=(env -i "PATH=$PATH" RUST_LOG=warn DWC_ENV=test DWC_DB_PATH="$WORK/load.db" DWC_COUNTRY=BR
  DWC_IDENTITY_PROVIDER=synthetic LEPTOS_SITE_ROOT="$ROOT/target/site" LEPTOS_OUTPUT_NAME=dwc
  LEPTOS_SITE_ADDR="127.0.0.1:$PORT")

cd "$ROOT"
"${app_env[@]}" "$BIN" >"$OUT/app.log" 2>&1 &
APP_PID=$!
for _ in $(seq 1 120); do
  curl -fsS "$HOST/healthz" >/dev/null 2>&1 && break
  kill -0 "$APP_PID" 2>/dev/null || { echo "app exited during startup (see $OUT/app.log)" >&2; exit 1; }
  sleep 0.5
done
curl -fsS "$HOST/healthz" >/dev/null || { echo "app not ready after 60 s" >&2; exit 1; }

echo "load: $USERS users for $DURATION against $HOST (release, disposable DB)"
(cd "$ROOT/tools/load" && cargo build --release --quiet)
set +e
DWC_LOAD_SUMMARY="$OUT/summary.json" "$ROOT/tools/load/target/release/dwc-load" \
  --host "$HOST" --users "$USERS" --hatch-rate "$USERS" --run-time "$DURATION" \
  --report-file "$OUT/report.html" --report-file "$OUT/report.json" \
  --no-reset-metrics --quiet >"$OUT/goose.log" 2>&1
GOOSE_STATUS=$?
set -e
[[ -s "$OUT/summary.json" ]] || { echo "goose produced no summary (exit $GOOSE_STATUS, see $OUT/goose.log)" >&2; exit 1; }

COUNTER_HTTP=$(curl -fsS -X POST -H "Origin: $HOST" -H 'Content-Type: application/x-www-form-urlencoded' "$HOST/api/get_counter")
kill "$APP_PID"; wait "$APP_PID" || true; APP_PID=""

# Invariants on the stopped instance: engine integrity, migrations, audit chain and
# the counter equal to the increments the server confirmed to clients.
set +e
"${app_env[@]}" "$BIN" check-db >"$OUT/check-db.json" 2>"$OUT/check-db.err"
CHECK_STATUS=$?
set -e
CONFIRMED=$(jq .increments_confirmed "$OUT/summary.json")
COUNTER_DB=$(jq .synthetic_counter "$OUT/check-db.json" 2>/dev/null || echo "null")
INVARIANTS=ok
[[ $CHECK_STATUS -eq 0 ]] || INVARIANTS="check-db failed"
[[ "$COUNTER_DB" == "$CONFIRMED" && "$COUNTER_HTTP" == "$CONFIRMED" ]] || INVARIANTS="counter db=$COUNTER_DB http=$COUNTER_HTTP confirmed=$CONFIRMED"

{
  echo "# Load experiment $(basename "$OUT")"
  echo
  echo "- commit: $(/usr/bin/git -C "$ROOT" rev-parse --short HEAD)$(/usr/bin/git -C "$ROOT" diff --quiet || echo ' (dirty)')"
  echo "- host: $(uname -srm), $(nproc) CPUs, $(awk '/MemTotal/ {printf "%.0f GiB", $2/1048576}' /proc/meminfo); generator on the same machine"
  echo "- users: $USERS, duration: $DURATION, mix: 95% reads / 5% writes, DWC_ENV=test, synthetic data"
  jq -r '"- requests: \(.total_requests), failures: \(.total_failures) (\(.failure_rate*100 | tostring | .[0:6])%), throughput: \(.throughput_rps | tostring | .[0:8]) req/s"' "$OUT/summary.json"
  echo
  echo "| request | n | fail | p50 ms | p95 ms | p99 ms | max ms |"
  echo "| --- | --- | --- | --- | --- | --- | --- |"
  jq -r '.requests | to_entries[] | "| \(.key) | \(.value.requests) | \(.value.failures) | \(.value.p50_ms) | \(.value.p95_ms) | \(.value.p99_ms) | \(.value.max_ms) |"' "$OUT/summary.json"
  echo
  echo "- invariants: $INVARIANTS (counter db=$COUNTER_DB, http=$COUNTER_HTTP, confirmed increments=$CONFIRMED; integrity=$(jq -c .integrity "$OUT/check-db.json" 2>/dev/null))"
  echo "- thresholds: $(jq -r 'if (.violations | length) == 0 then "ok" else (.violations | join("; ")) end' "$OUT/summary.json")"
} >"$OUT/summary.md"
cat "$OUT/summary.md"
echo "artifacts: $OUT"

[[ "$INVARIANTS" == ok ]] || { echo "invariants violated: $INVARIANTS" >&2; exit 1; }
[[ $GOOSE_STATUS -eq 0 ]] || { echo "goose exit $GOOSE_STATUS (thresholds or error)" >&2; exit 1; }
