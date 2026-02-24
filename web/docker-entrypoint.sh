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

# Use quoted heredoc to prevent shell expansion, then substitute the placeholder
cat > /usr/share/nginx/html/config.js <<'TEMPLATE'
window.__TC_ENV__ = {
  VITE_API_URL: "__VITE_API_URL__"
};
TEMPLATE

sed -i "s|__VITE_API_URL__|${VITE_API_URL}|" /usr/share/nginx/html/config.js

exec nginx -g 'daemon off;'
