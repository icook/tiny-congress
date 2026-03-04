#!/bin/sh
set -eu

# Generate runtime config from environment variables.
# This runs at container start so the same image works in any environment.
#
# Convention: any env var starting with TC_ or VITE_ is automatically
# included in window.__TC_ENV__. Adding a new frontend config value
# only requires setting the env var in the Helm deployment template —
# no changes to this script needed.

: "${VITE_API_URL:?VITE_API_URL must be set}"

# Validate URL format
case "$VITE_API_URL" in
  http://*|https://*) ;;
  *) echo "ERROR: VITE_API_URL must start with http:// or https://" >&2; exit 1 ;;
esac

# Write to /tmp so this works with readOnlyRootFilesystem (nginx serves via alias).
# Collect all TC_* and VITE_* env vars into a JSON object.
printf 'window.__TC_ENV__ = {\n' > /tmp/config.js
env | grep -E '^(TC_|VITE_)' | sort | sed 's/"/\\"/g; s/\(^[^=]*\)=\(.*\)/  \1: "\2",/' >> /tmp/config.js
printf '};\n' >> /tmp/config.js

exec nginx -g 'daemon off;'
