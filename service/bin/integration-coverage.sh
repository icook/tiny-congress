#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

export PATH="/usr/local/cargo/bin:${HOME}/.cargo/bin:${PATH}"

CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-/tmp/target}"
export CARGO_TARGET_DIR

DATABASE_URL_DEFAULT="postgres://postgres:postgres@postgres:5432/prioritization"
DATABASE_URL="${DATABASE_URL:-$DATABASE_URL_DEFAULT}"
export DATABASE_URL
: "${DATABASE_URL:?DATABASE_URL is required}"

EXPORT_LCOV_BASE64="${EXPORT_LCOV_BASE64:-1}"
ARTIFACTS_DIR="${ARTIFACTS_DIR:-coverage}"

mkdir -p "$ARTIFACTS_DIR"
# rm -rf /usr/src/app/target "$CARGO_TARGET_DIR"
cargo llvm-cov clean --workspace
cargo llvm-cov --lcov \
  --output-path "${ARTIFACTS_DIR}/backend-integration.lcov" \
  --remap-path-prefix \
  --test integration_tests \
  --no-clean \
  -- --test-threads=1 --nocapture

if [[ "${EXPORT_LCOV_BASE64:-0}" != "0" ]]; then
  marker_start="BEGIN_INTEGRATION_LCOV"
  marker_end="END_INTEGRATION_LCOV"
  echo "${marker_start}"
  gzip -c "${ARTIFACTS_DIR}/backend-integration.lcov" | base64 | tr -d '\n'
  echo
  echo "${marker_end}"
fi
