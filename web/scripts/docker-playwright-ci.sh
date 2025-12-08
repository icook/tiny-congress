#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
ARTIFACT_ROOT="$ROOT_DIR"

if [ -z "${IMAGE:-}" ]; then
  echo "IMAGE environment variable is required" >&2
  exit 1
fi

# Prepare artifact directories so docker bind mounts succeed even if the script deletes contents later.
mkdir -p "$ARTIFACT_ROOT/.nyc_output" \
  "$ARTIFACT_ROOT/coverage/playwright" \
  "$ARTIFACT_ROOT/reports" \
  "$ARTIFACT_ROOT/playwright-report" \
  "$ARTIFACT_ROOT/test-results"

docker run --rm \
  -e CI=true \
  -v "$ARTIFACT_ROOT/.nyc_output:/app/.nyc_output" \
  -v "$ARTIFACT_ROOT/coverage/playwright:/app/coverage/playwright" \
  -v "$ARTIFACT_ROOT/reports:/app/reports" \
  -v "$ARTIFACT_ROOT/playwright-report:/app/playwright-report" \
  -v "$ARTIFACT_ROOT/test-results:/app/test-results" \
  "$IMAGE" \
  /bin/sh -lc "cd /app && PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD=0 yarn playwright install --with-deps chromium && yarn test --watchAll=false && yarn playwright:ci"
