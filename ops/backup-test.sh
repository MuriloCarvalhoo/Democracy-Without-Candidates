#!/usr/bin/env bash
# End-to-end backup check on a disposable database (run via `just backup-test`):
# online snapshot while writes continue, offline snapshot of the stopped instance,
# encrypted archives, restore verification, retention and a tampered archive.
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
BIN="$ROOT/target/release/dwc"
[[ -x "$BIN" ]] || { echo "missing release build: run 'cargo leptos build --release'" >&2; exit 1; }
for tool in age age-keygen jq curl python3; do command -v "$tool" >/dev/null || { echo "missing: $tool" >&2; exit 1; }; done

WORK=$(mktemp -d)
APP_PID=""
WRITER_PID=""
cleanup() {
  local status=$?
  [[ $status -eq 0 || ! -f "$WORK/app.log" ]] || { echo "--- app log" >&2; tail -20 "$WORK/app.log" >&2; }
  for pid in "$WRITER_PID" "$APP_PID"; do
    if [[ -n "$pid" ]] && kill -0 "$pid" 2>/dev/null; then kill "$pid" 2>/dev/null || true; wait "$pid" 2>/dev/null || true; fi
  done
  if [[ -n "${DWC_KEEP_WORK:-}" ]]; then echo "kept $WORK" >&2; else rm -rf "$WORK"; fi
}
[[ -z "${DWC_TRACE:-}" ]] || set -x
trap cleanup EXIT
fail() { echo "FAIL: $*" >&2; exit 1; }

PORT=$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1])')
HOST="http://127.0.0.1:$PORT"
mkdir -p "$WORK/snapshots" "$WORK/backups"
age-keygen -o "$WORK/identity.txt" 2>/dev/null
age-keygen -y "$WORK/identity.txt" >"$WORK/recipients.txt"
export DWC_BIN="$BIN" DWC_BACKUP_DIR="$WORK/backups" DWC_BACKUP_RECIPIENTS="$WORK/recipients.txt" \
  DWC_BACKUP_IDENTITY="$WORK/identity.txt" DWC_SNAPSHOT_DIR="$WORK/snapshots"
export RUST_LOG=warn DWC_ENV=test DWC_DB_PATH="$WORK/instance.db" DWC_COUNTRY=BR DWC_IDENTITY_PROVIDER=synthetic \
  LEPTOS_SITE_ROOT="$ROOT/target/site" LEPTOS_OUTPUT_NAME=dwc LEPTOS_SITE_ADDR="127.0.0.1:$PORT"
cd "$ROOT"

increment() { curl -fsS -X POST -H "Origin: $HOST" -H 'Content-Type: application/x-www-form-urlencoded' "$HOST/api/increment_counter" >/dev/null; }

"$BIN" seed-dev >/dev/null
"$BIN" >"$WORK/app.log" 2>&1 &
APP_PID=$!
for _ in $(seq 1 120); do curl -fsS "$HOST/healthz" >/dev/null 2>&1 && break; sleep 0.5; done
curl -fsS "$HOST/healthz" >/dev/null || fail "app not ready"
for _ in $(seq 1 50); do increment; done

echo "== online snapshot while writes continue"
( while true; do increment || true; done ) &
WRITER_PID=$!
ONLINE=$(ops/backup.sh --online "$APP_PID")
kill "$WRITER_PID"; wait "$WRITER_PID" 2>/dev/null || true; WRITER_PID=""
ops/restore-test.sh "$ONLINE"
ONLINE_COUNTER=$(jq .synthetic_counter "${ONLINE%.db.age}.manifest.json")
[[ "$ONLINE_COUNTER" -ge 50 ]] || fail "online snapshot lost committed writes ($ONLINE_COUNTER < 50)"

FINAL=$(curl -fsS -X POST -H "Origin: $HOST" -H 'Content-Type: application/x-www-form-urlencoded' "$HOST/api/get_counter")
kill "$APP_PID"; wait "$APP_PID" || true; APP_PID=""

echo "== offline snapshot of the stopped instance (weekly copy + retention)"
for d in 01 02 03 04 05 06 07 08; do # older synthetic archives to exercise retention
  touch "$WORK/backups/daily/dwc-200001${d}T000000Z.db.age" "$WORK/backups/daily/dwc-200001${d}T000000Z.manifest.json"
done
for d in 01 02 03 04 05; do mkdir -p "$WORK/backups/weekly"; touch "$WORK/backups/weekly/dwc-200001${d}T000000Z.db.age"; done
sleep 1 # distinct archive name (second resolution)
OFFLINE=$(DWC_BACKUP_WEEKLY=1 ops/backup.sh --offline)
ops/restore-test.sh "$OFFLINE"
OFFLINE_COUNTER=$(jq .synthetic_counter "${OFFLINE%.db.age}.manifest.json")
[[ "$OFFLINE_COUNTER" == "$FINAL" ]] || fail "offline snapshot counter $OFFLINE_COUNTER != live $FINAL"
DAILY=$(ls "$WORK/backups/daily" | grep -c '\.db\.age$')
WEEKLY=$(ls "$WORK/backups/weekly" | grep -c '\.db\.age$')
[[ "$DAILY" == 7 && "$WEEKLY" == 4 ]] || fail "retention daily=$DAILY weekly=$WEEKLY (expected 7/4)"
[[ -f "$WORK/backups/daily/$(basename "$ONLINE")" && -f "$OFFLINE" ]] || fail "retention removed a recent archive"
! grep -q "SQLite\|CREATE TABLE" "$OFFLINE" || fail "archive is not encrypted"

echo "== tampered archive must fail"
cp "$OFFLINE" "$WORK/tampered.db.age"
printf '\x00\x00\x00\x00' | dd of="$WORK/tampered.db.age" bs=1 seek=400 conv=notrunc 2>/dev/null
if ops/restore-test.sh "$WORK/tampered.db.age" >/dev/null 2>&1; then fail "tampered archive accepted"; fi

echo "backup-test ok: online counter=$ONLINE_COUNTER (>=50 while writing), offline counter=$OFFLINE_COUNTER == live $FINAL, retention 7/4"
