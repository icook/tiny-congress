#!/bin/bash
# Pre-download GitHub Action archives into ACTIONS_RUNNER_ACTION_ARCHIVE_CACHE.
#
# The runner checks this directory for {owner}-{repo}-{sha}.tar.gz before
# downloading from GitHub, so baking these in eliminates per-job download
# overhead for all frequently-used actions.
#
# Usage (in Dockerfile via BuildKit secret):
#   RUN --mount=type=secret,id=github_token \
#       GITHUB_TOKEN=$(cat /run/secrets/github_token) \
#       CACHE_DIR=/opt/actions-archive-cache \
#       /tmp/cache-actions.sh
set -euo pipefail

CACHE_DIR="${CACHE_DIR:?CACHE_DIR must be set}"
mkdir -p "$CACHE_DIR"

# All action refs used across .github/workflows/ (on ARC runners).
# taiki-e/install-action uses per-tool tags as the action ref.
ACTIONS=(
  "actions/checkout@v4"
  "actions/checkout@v6"
  "actions/download-artifact@v7"
  "actions/github-script@v7"
  "actions/setup-node@v6"
  "actions/setup-python@v5"
  "actions/upload-artifact@v4"
  "actions/upload-artifact@v6"
  "aquasecurity/trivy-action@0.34.0"
  "docker/build-push-action@v6"
  "docker/login-action@v3"
  "docker/setup-buildx-action@v3"
  "dorny/paths-filter@v3"
  "dtolnay/rust-toolchain@stable"
  "EnricoMi/publish-unit-test-result-action@v2"
  "extractions/setup-just@v3"
  "gitleaks/gitleaks-action@v2"
  "helm/kind-action@v1.13.0"
  "marocchino/sticky-pull-request-comment@v2"
  "runs-on/cache@v4"
  "stackrox/kube-linter-action@v1"
  "Swatinem/rust-cache@v2"
  "taiki-e/install-action@cargo-deny"
  "taiki-e/install-action@cargo-llvm-cov"
  "taiki-e/install-action@cargo-machete"
  "taiki-e/install-action@v2"
  "taiki-e/install-action@wasm-pack"
)

AUTH_ARGS=()
if [[ -n "${GITHUB_TOKEN:-}" ]]; then
  AUTH_ARGS=(-H "Authorization: Bearer ${GITHUB_TOKEN}")
fi

cache_action() {
  local spec="$1"
  local repo="${spec%@*}"
  local ref="${spec#*@}"
  local owner="${repo%%/*}"
  local name="${repo#*/}"

  # Resolve the ref (tag or branch) to the underlying commit SHA.
  # The /commits/{ref} endpoint handles both annotated and lightweight tags.
  local sha
  sha=$(curl -fsSL "${AUTH_ARGS[@]}" \
    -H "Accept: application/vnd.github+json" \
    "https://api.github.com/repos/${repo}/commits/${ref}" \
    | python3 -c "import sys,json; print(json.load(sys.stdin)['sha'])")

  local outfile="${CACHE_DIR}/${owner}-${name}-${sha}.tar.gz"
  if [[ -f "$outfile" ]]; then
    printf '  hit  %-45s %s\n' "${spec}" "${sha:0:12}"
    return
  fi

  printf '  fetch %-45s %s\n' "${spec}" "${sha:0:12}"
  # Download tarball via API (follows redirect to codeload.github.com).
  curl -fsSL -L "${AUTH_ARGS[@]}" \
    "https://api.github.com/repos/${repo}/tarball/${sha}" \
    -o "$outfile"
}

echo "Pre-caching ${#ACTIONS[@]} GitHub Actions into ${CACHE_DIR} ..."
for action in "${ACTIONS[@]}"; do
  cache_action "$action"
done
echo "Done. $(du -sh "$CACHE_DIR" | cut -f1) total."
