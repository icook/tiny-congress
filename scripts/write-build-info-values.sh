#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
OUTPUT_PATH="${ROOT_DIR}/kube/app/values.build.yaml"

APP_VERSION="${APP_VERSION:-dev}"
GIT_SHA="${GIT_SHA:-unknown}"
BUILD_TIME="${BUILD_TIME:-$(date -u +"%Y-%m-%dT%H:%M:%SZ")}"
BUILD_MESSAGE="${BUILD_MESSAGE:-}"

cat >"$OUTPUT_PATH" <<EOF
buildInfo:
  appVersion: "${APP_VERSION}"
  gitSha: "${GIT_SHA}"
  buildTime: "${BUILD_TIME}"
  message: "${BUILD_MESSAGE}"
EOF

echo "Wrote build metadata to ${OUTPUT_PATH}"
