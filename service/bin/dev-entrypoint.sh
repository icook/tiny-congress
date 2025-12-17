#!/usr/bin/env bash
set -euo pipefail

APP_ROOT="${APP_ROOT:-/usr/src/app}"
cd "$APP_ROOT"

# Allow opt-out for environments that still run the pre-built binary
if [[ "${DISABLE_CARGO_WATCH:-0}" == "1" ]]; then
  exec tinycongress-api
fi

if ! command -v cargo-watch >/dev/null 2>&1; then
  echo "cargo-watch is not available on PATH" >&2
  exec tinycongress-api
fi

export RUST_LOG="${RUST_LOG:-info}"

echo "=== Watching for file changes in: $APP_ROOT ==="

# Watch core Rust sources and migrations, re-running the API binary on change
exec cargo watch \
  --watch src \
  --watch migrations \
  --watch Cargo.toml \
  --watch Cargo.lock \
  -x "run --bin tinycongress-api"
