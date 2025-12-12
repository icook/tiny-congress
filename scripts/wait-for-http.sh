#!/usr/bin/env bash
set -euo pipefail

if [ "$#" -lt 1 ]; then
  echo "Usage: $0 <url> [timeout_seconds]" >&2
  exit 1
fi

URL="$1"
TIMEOUT="${2:-60}"
SLEEP=2

end=$((SECONDS + TIMEOUT))

until [ $SECONDS -ge "$end" ]; do
  if curl -fsSL --max-time 5 "$URL" >/dev/null; then
    echo "Ready: $URL"
    exit 0
  fi

  sleep "$SLEEP"
done

echo "Timed out waiting for $URL" >&2
exit 1
