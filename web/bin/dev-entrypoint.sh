#!/usr/bin/env bash
set -euo pipefail

cd /app

# Prime Vite dependency optimizer so the first page load isn't slow.
echo "Prewarming Vite dependency cache..."
if ! yarn vite optimize; then
  echo "Vite prewarm failed; starting dev server anyway."
fi

exec yarn dev --host "${HOST:-0.0.0.0}" --port "${PORT:-5173}"
