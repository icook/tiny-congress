#!/bin/sh
set -eu

# Generate runtime config from environment variables.
# This runs at container start so the same image works in any environment.

: "${VITE_API_URL:?VITE_API_URL must be set}"

# Validate URL format
case "$VITE_API_URL" in
  http://*|https://*) ;;
  *) echo "ERROR: VITE_API_URL must start with http:// or https://" >&2; exit 1 ;;
esac

# Optional environment label for non-production badge (empty = production/hidden)
TC_ENVIRONMENT="${TC_ENVIRONMENT:-}"

# Optional demo verifier URL (empty = verifier UI hidden)
TC_DEMO_VERIFIER_URL="${TC_DEMO_VERIFIER_URL:-}"

# Write to /tmp so this works with readOnlyRootFilesystem (nginx serves via alias)
cat > /tmp/config.js <<'TEMPLATE'
window.__TC_ENV__ = {
  VITE_API_URL: "__VITE_API_URL__",
  TC_ENVIRONMENT: "__TC_ENVIRONMENT__",
  TC_DEMO_VERIFIER_URL: "__TC_DEMO_VERIFIER_URL__"
};
TEMPLATE

sed -i "s|__VITE_API_URL__|${VITE_API_URL}|" /tmp/config.js
sed -i "s|__TC_ENVIRONMENT__|${TC_ENVIRONMENT}|" /tmp/config.js
sed -i "s|__TC_DEMO_VERIFIER_URL__|${TC_DEMO_VERIFIER_URL}|" /tmp/config.js

exec nginx -g 'daemon off;'
