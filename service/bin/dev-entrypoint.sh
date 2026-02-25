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

export TC_LOGGING__LEVEL="${TC_LOGGING__LEVEL:-info}"

# Watch core Rust sources and migrations, re-running the API binary on change
# Paths are relative to workspace root (/usr/src/app)
exec cargo watch \
  --watch service/src \
  --watch service/migrations \
  --watch service/Cargo.toml \
  --watch Cargo.lock \
  -x "run --bin tinycongress-api"
