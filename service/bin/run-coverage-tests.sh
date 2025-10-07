#!/usr/bin/env bash
set -euo pipefail

COVERAGE_DIR="${COVERAGE_DIR:-$(pwd)/coverage}"
REPORTS_DIR="${REPORTS_DIR:-$(pwd)/reports}"

mkdir -p "${COVERAGE_DIR}" "${REPORTS_DIR}"
rm -f "${COVERAGE_DIR}/rust.lcov"

export PATH="${CARGO_BIN:-/usr/local/cargo/bin}:$PATH"
eval "$(cargo llvm-cov show-env --export-prefix)"
export RUSTC_BOOTSTRAP="${RUSTC_BOOTSTRAP:-1}"

TEST_TARGETS="${TEST_TARGETS:-api_tests graphql_tests model_tests}"
TEST_FLAGS="${TEST_FLAGS:--Z unstable-options --format json --report-time}"

read -r -a TARGET_ARR <<< "${TEST_TARGETS}"
TARGET_ARGS=()
for target in "${TARGET_ARR[@]}"; do
  TARGET_ARGS+=(--test "$target")
done

read -r -a FLAG_ARR <<< "${TEST_FLAGS}"

status=0
cargo test --locked "${TARGET_ARGS[@]}" -- "${FLAG_ARR[@]}" > "${REPORTS_DIR}/cargo-test.json" || status=$?

cargo2junit < "${REPORTS_DIR}/cargo-test.json" > "${REPORTS_DIR}/cargo-junit.xml"

if [ ${status} -eq 0 ]; then
  cargo llvm-cov report --lcov --output-path "${COVERAGE_DIR}/rust.lcov" --ignore-filename-regex '/usr/local/cargo'
fi

exit ${status}
