#!/usr/bin/env bash
set -euo pipefail

if [[ -z "${IMAGE:-}" ]]; then
  echo "IMAGE environment variable must be set by Skaffold" >&2
  exit 1
fi

REPO_ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd -P)"
COVERAGE_DIR="${REPO_ROOT}/service/coverage"
REPORTS_DIR="${REPO_ROOT}/service/reports"

mkdir -p "${COVERAGE_DIR}" "${REPORTS_DIR}"

docker run --rm \
  -v "${COVERAGE_DIR}:/tmp/coverage" \
  -v "${REPORTS_DIR}:/tmp/reports" \
  "${IMAGE}" /bin/bash -lc '
    set -euo pipefail
    echo "[skaffold] backend unit tests: starting"
    cd /usr/src/app
    COVERAGE_DIR=/tmp/coverage \
    REPORTS_DIR=/tmp/reports \
    LCOV_FILE=backend-unit.lcov \
    REPORT_BASENAME=backend-unit \
    TEST_TARGETS="api_tests graphql_tests model_tests" \
    TEST_FLAGS="--test-threads=1 --nocapture -Z unstable-options --format json --report-time" \
      bin/run-coverage-tests.sh
  '
