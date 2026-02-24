#!/bin/sh
set -eu

# Generate runtime config from environment variables.
# This runs at container start so the same image works in any environment.
cat > /usr/share/nginx/html/config.js <<EOF
window.__TC_ENV__ = {
  VITE_API_URL: "${VITE_API_URL:?VITE_API_URL must be set}"
};
EOF

exec nginx -g 'daemon off;'
