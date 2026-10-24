#!/usr/bin/env bash
# Consistent, verified, encrypted backup of one instance database.
#   ops/backup.sh --online <pid>   running server: SIGUSR1 -> engine VACUUM INTO in DWC_SNAPSHOT_DIR
#   ops/backup.sh --offline        stopped instance: `dwc backup` (the engine locks the DB file)
# Environment:
#   DWC_BACKUP_DIR         destination root (daily/ and weekly/ are created)
#   DWC_BACKUP_RECIPIENTS  age recipients file (public keys only; private key stays off the VPS)
#   DWC_BIN                server binary (default: target/release/dwc)
#   DWC_SNAPSHOT_DIR       online mode: same directory the server was started with
#   DWC_BACKUP_WEEKLY=1    also keep this archive as weekly (default: on Sundays)
#   DWC_* / LEPTOS_*       instance configuration, as for the server (offline mode and check-db)
# Retention: 7 daily, 4 weekly. Copying archives to separate storage is the operator's job.
set -euo pipefail
umask 077

ROOT=$(cd "$(dirname "$0")/.." && pwd)
BIN=${DWC_BIN:-$ROOT/target/release/dwc}
DEST=${DWC_BACKUP_DIR:?DWC_BACKUP_DIR is required}
RECIPIENTS=${DWC_BACKUP_RECIPIENTS:?DWC_BACKUP_RECIPIENTS is required}
command -v age >/dev/null || { echo "missing: age" >&2; exit 1; }
command -v jq >/dev/null || { echo "missing: jq" >&2; exit 1; }
[[ -x "$BIN" ]] || { echo "missing server binary $BIN" >&2; exit 1; }

WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
SNAP="$WORK/snapshot.db"

case "${1:-}" in
  --offline)
    "$BIN" backup "$SNAP" >/dev/null
    ;;
  --online)
    PID=${2:?usage: backup.sh --online <pid>}
    DIR=${DWC_SNAPSHOT_DIR:?DWC_SNAPSHOT_DIR is required for --online}
    before=$(ls "$DIR")
    kill -USR1 "$PID"
    new=""
    for _ in $(seq 1 600); do
      new=$(comm -13 <(echo "$before" | sort) <(ls "$DIR" | { grep -E '^dwc-.*\.db$' || true; } | sort) | tail -1)
      [[ -n "$new" ]] && break
      sleep 0.5
    done
    [[ -n "$new" ]] || { echo "no snapshot after 300 s (see server log)" >&2; exit 1; }
    mv "$DIR/$new" "$SNAP"
    ;;
  *)
    echo "usage: backup.sh --online <pid> | --offline" >&2
    exit 2
    ;;
esac

# Verify the copy before keeping it: integrity, migrations, audit chain, row counts.
DWC_DB_PATH="$SNAP" "$BIN" check-db >"$WORK/manifest.json"

NAME="dwc-$(date -u +%Y%m%dT%H%M%SZ)"
mkdir -p "$DEST/daily" "$DEST/weekly"
age -R "$RECIPIENTS" -o "$DEST/daily/$NAME.db.age" "$SNAP"
cp "$WORK/manifest.json" "$DEST/daily/$NAME.manifest.json"
if [[ "${DWC_BACKUP_WEEKLY:-}" == 1 || "$(date -u +%u)" == 7 ]]; then
  cp "$DEST/daily/$NAME.db.age" "$DEST/daily/$NAME.manifest.json" "$DEST/weekly/"
fi

prune() { # keep the newest $2 archives (names sort by UTC timestamp)
  ls "$1" | { grep -E '^dwc-.*\.db\.age$' || true; } | sort -r | tail -n +"$(($2 + 1))" | while read -r old; do
    rm -f "$1/$old" "$1/${old%.db.age}.manifest.json"
  done
}
prune "$DEST/daily" 7
prune "$DEST/weekly" 4
echo "$DEST/daily/$NAME.db.age"
