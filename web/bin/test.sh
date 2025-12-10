#!/bin/bash
set -euo pipefail

cd /app
export CI=true

yarn test
