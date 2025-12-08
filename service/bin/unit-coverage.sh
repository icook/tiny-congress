#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

export PATH="/usr/local/cargo/bin:${HOME}/.cargo/bin:${PATH}"

CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-/tmp/target}"
export CARGO_TARGET_DIR

mkdir -p coverage
rm -rf /usr/src/app/target "$CARGO_TARGET_DIR"
cargo llvm-cov clean --workspace
cargo llvm-cov --lcov \
  --output-path coverage/backend-unit.lcov \
  --remap-path-prefix \
  --test api_tests \
  --test graphql_tests \
  --test model_tests \
  -- --test-threads=1 --nocapture
