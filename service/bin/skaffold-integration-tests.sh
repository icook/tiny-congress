#!/usr/bin/env bash
set -euo pipefail

if [[ -z "${IMAGE:-}" ]]; then
  echo "IMAGE environment variable must be set by Skaffold" >&2
  exit 1
fi

REPO_ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd -P)"
COVERAGE_DIR="${REPO_ROOT}/service/coverage"
REPORTS_DIR="${REPO_ROOT}/service/reports"
PORT="75432"

mkdir -p "${COVERAGE_DIR}" "${REPORTS_DIR}"

kubectl port-forward svc/postgres "${PORT}:5432" >/tmp/postgres-port-forward.log 2>&1 &
PF_PID=$!
trap 'kill ${PF_PID}' EXIT
echo "[skaffold] backend integration tests: waiting for port-forward on ${PORT}"
python - "${PORT}" <<'PY'
import socket, sys, time
port = int(sys.argv[1])
for _ in range(60):
    try:
        with socket.create_connection(("127.0.0.1", port), timeout=1):
            break
    except OSError:
        time.sleep(1)
else:
    sys.exit("Timed out waiting for Postgres port-forward")
PY

docker run --rm --network host \
  -v "${COVERAGE_DIR}:/tmp/coverage" \
  -v "${REPORTS_DIR}:/tmp/reports" \
  "${IMAGE}" /bin/bash -lc '
    set -euo pipefail
    echo "[skaffold] backend integration tests: starting"
    cd /usr/src/app
    COVERAGE_DIR=/tmp/coverage \
    REPORTS_DIR=/tmp/reports \
    LCOV_FILE=backend-integration.lcov \
    REPORT_BASENAME=backend-integration \
    DATABASE_URL=postgres://postgres:postgres@127.0.0.1:'"${PORT}"'/prioritization \
    TEST_TARGETS="integration_tests" \
    TEST_FLAGS="--test-threads=1 --nocapture -Z unstable-options --format json --report-time" \
      bin/run-coverage-tests.sh
  '
