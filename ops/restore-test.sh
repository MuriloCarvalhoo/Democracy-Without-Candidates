#!/usr/bin/env bash
# Restores an encrypted archive into an isolated temporary path and verifies it with
# the application (`dwc check-db`: integrity, migrations known to this binary, audit
# chain, row counts), then compares with the manifest written at backup time.
#   ops/restore-test.sh <archive.db.age>
# Environment: DWC_BACKUP_IDENTITY (age identity file), DWC_BIN (default target/release/dwc).
# Never touches the live database; the restored copy is deleted afterwards.
set -euo pipefail

ARCHIVE=${1:?usage: restore-test.sh <archive.db.age>}
IDENTITY=${DWC_BACKUP_IDENTITY:?DWC_BACKUP_IDENTITY is required}
ROOT=$(cd "$(dirname "$0")/.." && pwd)
BIN=${DWC_BIN:-$ROOT/target/release/dwc}
MANIFEST="${ARCHIVE%.db.age}.manifest.json"

WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
age -d -i "$IDENTITY" -o "$WORK/restored.db" "$ARCHIVE"

env -i "PATH=$PATH" RUST_LOG=warn DWC_ENV=test DWC_DB_PATH="$WORK/restored.db" DWC_COUNTRY=BR \
  DWC_IDENTITY_PROVIDER=synthetic LEPTOS_OUTPUT_NAME=dwc LEPTOS_SITE_ADDR=127.0.0.1:0 \
  "$BIN" check-db >"$WORK/report.json"

if [[ -f "$MANIFEST" ]]; then
  if ! diff <(jq -S . "$MANIFEST") <(jq -S . "$WORK/report.json") >/dev/null; then
    echo "restored database differs from manifest $MANIFEST" >&2
    diff <(jq -S . "$MANIFEST") <(jq -S . "$WORK/report.json") >&2 || true
    exit 1
  fi
fi
echo "restore ok: $(jq -c '{integrity, migrations_applied, audit_chain: (.audit_chain | split(":")[0:2] | join(":")), synthetic_counter, users: .tables.users}' "$WORK/report.json")"
