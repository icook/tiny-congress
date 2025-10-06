#!/usr/bin/env bash
set -euo pipefail

COVERAGE_DIR="${COVERAGE_DIR:-/workspace/coverage}"
REPORTS_DIR="${REPORTS_DIR:-/workspace/reports}"

mkdir -p "${COVERAGE_DIR}" "${REPORTS_DIR}"
rm -f "${COVERAGE_DIR}/rust.lcov"

export PATH="${CARGO_BIN:-/usr/local/cargo/bin}:$PATH"
eval "$(cargo llvm-cov show-env --export-prefix)"
export RUSTC_BOOTSTRAP="${RUSTC_BOOTSTRAP:-1}"

status=0
cargo test --locked \
  --test api_tests \
  --test graphql_tests \
  --test model_tests \
  -- -Z unstable-options --format json --report-time > "${REPORTS_DIR}/cargo-test.json" || status=$?

cargo2junit < "${REPORTS_DIR}/cargo-test.json" > "${REPORTS_DIR}/cargo-junit.xml"

if [ ${status} -eq 0 ]; then
  cargo llvm-cov report --lcov --output-path "${COVERAGE_DIR}/rust.lcov" --ignore-filename-regex '/usr/local/cargo'
fi

exit ${status}
