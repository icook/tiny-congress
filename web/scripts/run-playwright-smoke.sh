#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"

: "${PLAYWRIGHT_BASE_URL:=http://127.0.0.1:5173}"
: "${PLAYWRIGHT_API_URL:=http://127.0.0.1:8080/graphql}"

export PLAYWRIGHT_SKIP_WEB_SERVER=true
export PLAYWRIGHT_BASE_URL
export PLAYWRIGHT_API_URL
export CI="${CI:-true}"

cd "$ROOT_DIR"
yarn playwright:test --grep @smoke "$@"
