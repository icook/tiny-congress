#!/usr/bin/env bash
set -euo pipefail

COVERAGE_DIR="${COVERAGE_DIR:-/workspace/coverage}"
REPORTS_DIR="${REPORTS_DIR:-/workspace/reports}"
TARGET_LLVM_DIR="${TARGET_LLVM_DIR:-/usr/src/app/target/llvm-cov-target}"
PROFILE_DIR="${PROFILE_DIR:-${TARGET_LLVM_DIR}/profiles}"

mkdir -p "${COVERAGE_DIR}" "${REPORTS_DIR}"
rm -f "${COVERAGE_DIR}/rust.lcov"

rm -rf "${TARGET_LLVM_DIR}"
mkdir -p "${PROFILE_DIR}"

export PATH="${CARGO_BIN:-/usr/local/cargo/bin}:$PATH"
export LLVM_PROFILE_FILE="${PROFILE_DIR}/coverage-%p-%m.profraw"
DEFAULT_FLAGS="-C instrument-coverage -C link-dead-code -C overflow-checks=off"
export RUSTFLAGS="${RUSTFLAGS:-$DEFAULT_FLAGS}"
export RUSTDOCFLAGS="${RUSTDOCFLAGS:-$DEFAULT_FLAGS}"
export RUST_TEST_THREADS="${RUST_TEST_THREADS:-1}"
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
