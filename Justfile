# Thin command interface used locally and in CI. See docs/architecture/foundation-setup.md.

set shell := ["bash", "-euo", "pipefail", "-c"]

dev_env := "DWC_ENV=development DWC_DB_PATH=data/dev.db DWC_COUNTRY=BR DWC_IDENTITY_PROVIDER=synthetic"

default:
    @just --list

# Verify required tools; fails explicitly instead of skipping.
doctor:
    @command -v cargo >/dev/null || { echo "missing: cargo"; exit 1; }
    @command -v cargo-leptos >/dev/null || { echo "missing: cargo-leptos (cargo install cargo-leptos --locked --version 0.3.11)"; exit 1; }
    @rustup target list --installed | grep -q wasm32-unknown-unknown || { echo "missing: rustup target add wasm32-unknown-unknown"; exit 1; }
    @echo "toolchain ok: $(rustc --version), $(cargo leptos --version)"

# Run the app on 127.0.0.1:3000 with the persistent development database (never reset).
dev: doctor
    {{dev_env}} cargo leptos watch

# Enable synthetic development fixtures (flags, demo accounts) in data/dev.db.
dev-seed:
    cargo leptos build
    {{dev_env}} LEPTOS_SITE_ROOT=target/site target/debug/dwc seed-dev

# Formatting, lint and compilation of the server and WASM targets.
check: doctor
    cargo fmt --check
    cargo clippy --features ssr --all-targets -- -D warnings
    cargo clippy --features hydrate --lib --target wasm32-unknown-unknown -- -D warnings

# Rules and persistence tests with the real TursoDB engine in disposable databases.
test:
    cargo test --features ssr

# Browser regressions: isolated app + chromedriver, bounded waits.
e2e: doctor
    @command -v ${CHROMEDRIVER:-chromedriver} >/dev/null || { echo "missing: chromedriver matching the installed Chrome"; exit 1; }
    cargo leptos build
    mkdir -p target/e2e && cp target/debug/dwc target/e2e/dwc
    DWC_E2E_BIN=target/e2e/dwc cargo test --features ssr --test e2e --test e2e_proposals -- --ignored --test-threads=2

# Release build used by load tests and deployment.
release:
    cargo leptos build --release

# Goose load smoke against an isolated release instance (synthetic data).
load users="10" duration="60s": release
    tools/load/run.sh {{users}} {{duration}}
