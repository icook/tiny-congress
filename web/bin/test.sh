#!/bin/bash
set -euo pipefail

cd /app
export CI=true

PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD=0 yarn playwright install --with-deps chromium
yarn test --watchAll=false
yarn playwright:ci
