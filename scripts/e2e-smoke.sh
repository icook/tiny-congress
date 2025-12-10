#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
PROFILE="${SKAFFOLD_PROFILE:-}"
ARTIFACTS_FILE="${SKAFFOLD_ARTIFACTS_FILE:-}"
ARTIFACTS_FLAG=()

if [ -n "$ARTIFACTS_FILE" ]; then
  ARTIFACTS_FLAG=(--build-artifacts "$ARTIFACTS_FILE")
fi

APP_VERSION="${APP_VERSION:-$(git -C "$ROOT_DIR" rev-parse --short HEAD 2>/dev/null || echo dev)}"
GIT_SHA="${GIT_SHA:-$(git -C "$ROOT_DIR" rev-parse HEAD 2>/dev/null || echo unknown)}"
BUILD_TIME="${BUILD_TIME:-$(date -u +"%Y-%m-%dT%H:%M:%SZ")}"

echo "Stamping build info:"
echo "  APP_VERSION=${APP_VERSION}"
echo "  GIT_SHA=${GIT_SHA}"
echo "  BUILD_TIME=${BUILD_TIME}"
APP_VERSION="$APP_VERSION" GIT_SHA="$GIT_SHA" BUILD_TIME="$BUILD_TIME" "$ROOT_DIR/scripts/write-build-info-values.sh"

cleanup() {
  for pid_file in /tmp/pf-api.pid /tmp/pf-ui.pid; do
    if [ -f "$pid_file" ]; then
      pid=$(cat "$pid_file")
      kill "$pid" || true
      rm -f "$pid_file"
    fi
  done

  if [ "${SKIP_TEARDOWN:-0}" != "1" ]; then
    echo "Tearing down Skaffold deployment..."
    skaffold delete ${PROFILE:+-p "$PROFILE"} "${ARTIFACTS_FLAG[@]}" || true
  else
    echo "SKIP_TEARDOWN=1; leaving cluster resources intact."
  fi
}
trap cleanup EXIT

echo "Running skaffold run..."
skaffold run ${PROFILE:+-p "$PROFILE"} "${ARTIFACTS_FLAG[@]}"

echo "Waiting for deployments to roll out..."
kubectl rollout status deployment/tc --timeout=120s
kubectl rollout status deployment/tc-frontend --timeout=120s

echo "Starting port-forwards..."
kubectl port-forward service/tc 8080:8080 >/tmp/pf-api.log 2>&1 &
echo $! >/tmp/pf-api.pid
kubectl port-forward service/tc-frontend 5173:5173 >/tmp/pf-ui.log 2>&1 &
echo $! >/tmp/pf-ui.pid
sleep 5

echo "Waiting for HTTP readiness..."
"$ROOT_DIR/scripts/wait-for-http.sh" http://127.0.0.1:8080/health 120
"$ROOT_DIR/scripts/wait-for-http.sh" http://127.0.0.1:8080/graphql 120
"$ROOT_DIR/scripts/wait-for-http.sh" http://127.0.0.1:5173 180

echo "Running Playwright smoke (@smoke)..."
PLAYWRIGHT_BASE_URL=http://127.0.0.1:5173 \
  PLAYWRIGHT_API_URL=http://127.0.0.1:8080/graphql \
  "$ROOT_DIR/web/scripts/run-playwright-smoke.sh"

echo "Smoke test completed."
