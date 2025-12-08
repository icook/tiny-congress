#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

DATABASE_URL_DEFAULT="postgres://postgres:postgres@postgres:5432/prioritization"
DATABASE_URL="${DATABASE_URL:-$DATABASE_URL_DEFAULT}"
export DATABASE_URL
: "${DATABASE_URL:?DATABASE_URL is required}"

EXPORT_LCOV_BASE64="${EXPORT_LCOV_BASE64:-1}"

mkdir -p coverage
cargo llvm-cov clean --workspace
cargo llvm-cov --lcov \
  --output-path coverage/backend-integration.lcov \
  --remap-path-prefix \
  --test integration_tests \
  -- --test-threads=1 --nocapture

if [[ "${EXPORT_LCOV_BASE64:-0}" != "0" ]]; then
  echo "BEGIN_INTEGRATION_LCOV"
  base64 coverage/backend-integration.lcov | tr -d '\n'
  echo
  echo "END_INTEGRATION_LCOV"
fi
