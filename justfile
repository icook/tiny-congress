# TinyCongress Development Toolchain
#
# Quick Start:
#   1. just setup           # Check prerequisites (one-time)
#   2. just dev             # Full-stack dev (requires Skaffold + KinD cluster)
#
# Daily commands (no cluster needed):
#   just lint               # Lint all code
#   just fmt                # Fix all formatting
#   just test               # Unit tests (backend + frontend + wasm)
#   just test-backend       # Backend unit tests only
#   just build              # Build everything locally
#   just dev-backend        # Backend with hot reload
#   just dev-frontend       # Vite dev server
#   just dev-storybook      # Storybook dev server
#   just codegen            # Regenerate GraphQL + REST types
#
# Full-stack (requires Docker + KinD):
#   just test-ci            # Build images + run all tests via Skaffold
#   just kind-create        # Create local KinD cluster (one-time)
#
# Run `just --list` for complete recipe list

# Default recipe - show help
default:
    @just --list

# =============================================================================
# Backend (Rust) Commands
# =============================================================================

# Run backend unit tests (auto-builds postgres image if needed for DB tests)
test-backend: _ensure-test-postgres prune-testcontainers
    cd service && cargo test

# Run backend unit tests in watch mode (re-runs on file changes)
test-backend-watch: _ensure-test-postgres
    cd service && cargo watch -x test

# Run backend unit tests with coverage (generates LCOV for unified report)
test-backend-cov: _ensure-test-postgres
    mkdir -p service/coverage
    cd service && cargo llvm-cov --lcov --output-path coverage/backend-unit.lcov

# Internal: ensure postgres image exists for testcontainers
_ensure-test-postgres:
    #!/usr/bin/env bash
    if ! docker image inspect tc-postgres:local >/dev/null 2>&1; then
        echo "Building tc-postgres:local image for testcontainers..."
        docker build -t tc-postgres:local -f dockerfiles/Dockerfile.postgres dockerfiles/
    fi

# Check backend formatting
lint-backend-fmt:
    cd service && cargo fmt --all -- --check

# Run clippy linter on backend
lint-backend-clippy:
    cd service && cargo clippy --all-features -- -D warnings

# Run all backend linting (format + clippy)
lint-backend: lint-backend-fmt lint-backend-clippy

# Fix backend formatting
fmt-backend:
    cd service && cargo fmt --all

# Build backend (debug)
build-backend:
    cd service && cargo build

# Build backend (release)
build-backend-release:
    cd service && cargo build --release

# Run backend dev server with hot reload
dev-backend:
    cd service && cargo watch --watch src --watch migrations -x "run --bin tinycongress-api"

# =============================================================================
# WASM (Crypto Library)
# =============================================================================

# Build crypto-wasm for production (outputs to web/src/wasm/tc-crypto/)
build-wasm:
    @echo "Building tc-crypto WASM for production..."
    cd crates/tc-crypto && wasm-pack build --target web --release --out-dir ../../web/src/wasm/tc-crypto
    @echo "✓ WASM built to web/src/wasm/tc-crypto/"

# Internal: Build crypto-wasm for development (faster, debug symbols)
_build-wasm-dev:
    @echo "Building tc-crypto WASM for development..."
    cd crates/tc-crypto && wasm-pack build --target web --dev --out-dir ../../web/src/wasm/tc-crypto
    @echo "✓ WASM built to web/src/wasm/tc-crypto/"

# Test crypto-wasm (native Rust tests)
test-wasm:
    cargo test -p tc-crypto

# Internal: Clean WASM build artifacts
_clean-wasm:
    rm -rf crates/tc-crypto/pkg web/src/wasm/tc-crypto
    @echo "✓ WASM artifacts cleaned"

# =============================================================================
# Code Generation (GraphQL & OpenAPI Types)
# =============================================================================

# Internal: Export GraphQL schema from Rust backend
_export-schema:
    cd service && cargo run --bin export_schema > ../web/schema.graphql

# Internal: Export OpenAPI schema from Rust backend
_export-openapi:
    cd service && cargo run --bin export_openapi > ../web/openapi.json

# Generate TypeScript types and Zod schemas from GraphQL schema
codegen-graphql:
    cd web && yarn graphql-codegen

# Generate TypeScript types from OpenAPI schema
codegen-openapi:
    cd web && yarn openapi-typescript openapi.json -o src/api/generated/rest.ts && yarn prettier --write src/api/generated/rest.ts

# Full codegen: export schemas from Rust + generate TypeScript types
codegen: _export-schema _export-openapi codegen-graphql codegen-openapi
    @echo "✓ GraphQL and REST types generated"

# =============================================================================
# Frontend (React/TypeScript) Commands
# =============================================================================

# Run frontend unit tests (requires WASM artifacts)
test-frontend: _ensure-frontend-deps _build-wasm-dev
    cd web && yarn vitest

# Run frontend unit tests in watch mode (requires WASM artifacts)
test-frontend-watch: _ensure-frontend-deps _build-wasm-dev
    cd web && yarn vitest:watch

# Run full frontend test suite (typecheck + lint + vitest + build) - includes E2E via CI
test-frontend-full: _ensure-frontend-deps _build-wasm-dev
    cd web && yarn test

# Run frontend E2E tests with Playwright (requires running backend/frontend)
test-frontend-e2e:
    cd web && yarn playwright:ci

# Internal: Check frontend types
_typecheck-frontend:
    cd web && yarn typecheck

# Run all frontend linting (prettier + eslint + stylelint)
lint-frontend: _ensure-frontend-deps
    cd web && yarn lint

# Run eslint only
lint-frontend-eslint:
    cd web && yarn eslint

# Run stylelint only
lint-frontend-stylelint:
    cd web && yarn stylelint '**/*.css'

# Check frontend formatting with prettier
lint-frontend-prettier:
    cd web && yarn prettier

# Fix frontend formatting with prettier
fmt-frontend:
    cd web && yarn prettier:write

# Build frontend for production
build-frontend: _ensure-frontend-deps
    cd web && yarn build

# Run frontend dev server
dev-frontend: _ensure-frontend-deps
    cd web && yarn dev

# Run Storybook dev server
dev-storybook:
    cd web && yarn storybook

# Build Storybook static site
build-storybook:
    cd web && yarn storybook:build

# Internal: Install frontend dependencies if yarn.lock is newer than node_modules
_ensure-frontend-deps:
    #!/usr/bin/env bash
    set -euo pipefail
    ROOT="{{justfile_directory()}}"
    STAMP="$ROOT/web/node_modules/.ts-yarn-install"
    if [ ! -f "$STAMP" ] || [ "$ROOT/web/yarn.lock" -nt "$STAMP" ]; then
        echo "Installing frontend dependencies..."
        cd "$ROOT/web" && yarn install
        touch "$STAMP"
    fi

# =============================================================================
# Full-Stack Development (Requires Skaffold + Kubernetes Cluster)
# =============================================================================

# Internal: Check that local rustc version matches mise.toml (for shared cargo cache)
_check-rust-version:
    #!/usr/bin/env bash
    set -euo pipefail
    EXPECTED=$(grep 'rust' mise.toml | sed 's/.*"\(.*\)"/\1/')
    # Use mise exec if available, otherwise fall back to direct rustc
    if command -v mise &>/dev/null; then
        ACTUAL=$(mise exec -- rustc --version | awk '{print $2}')
    else
        ACTUAL=$(rustc --version | awk '{print $2}')
    fi
    if [[ "$ACTUAL" != "$EXPECTED" ]]; then
        echo "ERROR: rustc version mismatch!"
        echo "  Expected: $EXPECTED (from mise.toml)"
        echo "  Actual:   $ACTUAL"
        echo ""
        echo "Run: mise install"
        echo "Or:  rustup install $EXPECTED && rustup default $EXPECTED"
        exit 1
    fi
    echo "rustc version OK: $ACTUAL"

# Create KinD cluster with shared cargo cache mount
# Run this once before `just dev` for faster Rust rebuilds
kind-create:
    @echo "Creating KinD cluster with cargo cache mount..."
    kind create cluster --config kind-config.yaml
    @echo "Cluster ready. Run 'just dev' to start development."

# Delete KinD cluster
kind-delete:
    kind delete cluster

# Extract rust version from mise.toml (single source of truth)
# Fail loud if mise.toml is missing or malformed
# Exported so skaffold's {{.RUST_VERSION}} template resolves automatically.
export RUST_VERSION := ```
    VERSION=$(grep 'rust' mise.toml 2>/dev/null | sed 's/.*"\(.*\)"/\1/')
    if [ -z "$VERSION" ]; then
        echo "ERROR: Could not extract rust version from mise.toml" >&2
        echo "Ensure mise.toml exists with: rust = \"X.Y.Z\"" >&2
        exit 1
    fi
    echo "$VERSION"
```

# Start full-stack dev environment with Skaffold (requires KinD cluster)
dev: _check-rust-version
    @echo "Starting full-stack dev with Skaffold (targeting KinD cluster)..."
    @echo "Prerequisites: run 'just kind-create' first for KinD with shared cargo cache"
    skaffold dev --kube-context kind-kind --port-forward --cleanup=false --skip-tests

# Build all container images (for current profile)
build-images:
    @echo "Building container images..."
    skaffold build

# Internal: Build images and output artifacts JSON (for reuse with test-ci)
_build-images-artifacts:
    @echo "Building container images and writing artifacts..."
    skaffold build --file-output artifacts.json

# Deploy dev images to local KinD cluster
kind-deploy:
    @echo "Deploying to KinD cluster (dev profile)..."
    skaffold run -p dev

# Deploy release images to local KinD cluster
kind-deploy-release:
    @echo "Deploying to KinD cluster (release profile)..."
    skaffold run -p release

# Remove all deployed resources from local KinD cluster
kind-undeploy:
    @echo "Cleaning up KinD resources..."
    skaffold delete

# =============================================================================
# Security & Dependency Audit
# =============================================================================

# Run all security and hygiene checks
audit: audit-deps audit-secrets audit-unused
    @echo "✓ All security checks passed"

# Check for vulnerabilities and license issues (cargo-deny + yarn audit)
audit-deps:
    cd service && cargo deny check
    cd web && yarn npm audit --severity high
    @echo "✓ Dependency audit passed"

# Check for leaked secrets (requires gitleaks: brew install gitleaks)
audit-secrets:
    gitleaks detect --source . --verbose
    @echo "✓ No secrets detected"

# Check for unused Rust dependencies (requires cargo-machete: cargo install cargo-machete)
audit-unused:
    cd service && cargo machete
    @echo "✓ No unused dependencies"

# =============================================================================
# Quality Checks (Local - No Cluster Required)
# =============================================================================

# Run all linting (backend + frontend) - no cluster required
lint: lint-backend lint-frontend lint-typecheck
    @echo "✓ All linting passed"

# Type check frontend (TypeScript)
lint-typecheck: _typecheck-frontend

# =============================================================================
# Static Analysis Tools
# =============================================================================

# Run all static analysis checks
lint-static: lint-typos lint-dockerfiles lint-workflows lint-scripts
    @echo "✓ All static analysis passed"

# Check for anti-patterns that shared utilities should replace
lint-patterns:
    #!/usr/bin/env bash
    set -euo pipefail
    FAIL=0

    # 1. Inline ErrorResponse for status codes that have shared helpers
    #    (BAD_REQUEST, NOT_FOUND, UNAUTHORIZED, INTERNAL_SERVER_ERROR)
    #    Domain-specific mappers using CONFLICT, TOO_MANY_REQUESTS, etc. are fine.
    VIOLATIONS=$(rg 'StatusCode::(BAD_REQUEST|NOT_FOUND|UNAUTHORIZED|INTERNAL_SERVER_ERROR).*\n.*ErrorResponse' \
         service/src/ \
         --glob '!service/src/http/mod.rs' \
         --glob '!**/tests/**' \
         --glob '!**/test*' \
         --multiline \
         -l 2>/dev/null || true)
    # Exclude files that contain a "lint-patterns:allow-inline-error" marker
    for f in $VIOLATIONS; do
        if ! grep -q 'lint-patterns:allow-inline-error' "$f" 2>/dev/null; then
            echo "  $f"
            FAIL=1
        fi
    done
    if [ "$FAIL" -eq 1 ]; then
        echo "FAIL: Inline ErrorResponse construction for standard status codes."
        echo "  Use crate::http::{bad_request,not_found,unauthorized,internal_error} instead."
        echo "  See AGENTS.md 'Shared Utilities' section."
    fi

    # 2. serde_json::json! used for error responses in handlers
    if rg 'serde_json::json!\(\s*\{\s*"error"' service/src/ \
         --glob '!**/tests/**' \
         --glob '!**/test*' \
         -l 2>/dev/null; then
        echo "FAIL: serde_json::json! used for error responses."
        echo "  Use crate::http::ErrorResponse struct instead."
        FAIL=1
    fi

    # 3. Raw PgPool in HTTP handlers outside identity repo
    #    Handlers should use Arc<dyn *Repo> or Arc<dyn *Service>, not raw pool.
    #    Allowed: identity/http/ (owns the repo), repo/ modules, main.rs, tests.
    POOL_VIOLATIONS=$(rg 'Extension<PgPool>' service/src/ \
         --glob '!service/src/identity/**' \
         --glob '!service/src/**/repo/**' \
         --glob '!service/src/main.rs' \
         --glob '!service/src/app_builder.rs' \
         --glob '!**/tests/**' \
         --glob '!**/test*' \
         -l 2>/dev/null || true)
    for f in $POOL_VIOLATIONS; do
        if ! grep -q 'lint-patterns:allow-raw-pool' "$f" 2>/dev/null; then
            echo "  $f"
            POOL_FAIL=1
        fi
    done
    if [ "${POOL_FAIL:-0}" -eq 1 ]; then
        echo "FAIL: Raw PgPool used in HTTP handler outside identity."
        echo "  Use Arc<dyn *Repo> or Arc<dyn *Service> instead."
        echo "  See AGENTS.md 'Shared Utilities' section."
        FAIL=1
    fi

    if [ "$FAIL" -eq 0 ]; then
        echo "✓ No anti-patterns found"
    else
        exit 1
    fi

# Check for typos in code and docs (requires typos: cargo install typos-cli)
lint-typos:
    typos

# Lint Dockerfiles (requires hadolint: brew install hadolint)
lint-dockerfiles:
    hadolint service/Dockerfile service/Dockerfile.dev web/Dockerfile web/Dockerfile.dev dockerfiles/Dockerfile.postgres

# Lint GitHub Actions workflows (requires actionlint: brew install actionlint)
lint-workflows:
    actionlint

# Lint shell scripts (requires shellcheck: brew install shellcheck)
lint-scripts:
    shellcheck web/bin/*.sh web/scripts/*.sh service/bin/*.sh scripts/*.sh

# Lint Kubernetes manifests (requires kube-linter: brew install kube-linter)
lint-kube:
    kube-linter lint kube/ --config .kube-linter.yaml

# Generate Grafana dashboard JSON from Python definitions
generate-dashboards:
    cd kube/dashboards && python3 generate.py

# Validate dashboard definitions without writing output
check-dashboards:
    cd kube/dashboards && python3 generate.py --check

# Check for rustdoc warnings (broken links)
lint-docs:
    cd service && cargo doc --no-deps --document-private-items 2>&1 | (! grep -E "^warning:")

# Fix all formatting (backend + frontend)
fmt: fmt-backend fmt-frontend
    @echo "✓ All formatting applied"

# Run all local unit tests (backend + frontend + wasm)
test: test-backend test-wasm test-frontend
    @echo "✓ Unit tests passed"

# Run frontend unit tests with coverage
test-frontend-cov: _ensure-frontend-deps _build-wasm-dev
    cd web && yarn vitest:coverage

# Run all unit tests with coverage
test-cov: test-backend-cov test-frontend-cov
    @echo "✓ Coverage reports generated"

# Build everything locally (no images)
build: build-backend build-wasm build-frontend
    @echo "✓ All builds successful"

# Build everything in release mode
build-release: build-backend-release build-wasm build-frontend
    @echo "✓ Release builds successful"

# =============================================================================
# Full-Stack Testing (Requires Docker + Kubernetes)
# =============================================================================

# Run full test suite via Skaffold (mirrors CI - RECOMMENDED per AGENTS.md)
test-ci: _build-images-artifacts
    @echo "Running full test suite via Skaffold..."
    skaffold test --build-artifacts artifacts.json

# =============================================================================
# Automated Refinement
# =============================================================================

# Run automated refinement loop (reads refine.toml for focus area)
refine:
    ./scripts/refine.sh

# Dry-run refinement (show generated prompt without executing)
refine-dry-run:
    ./scripts/refine.sh --dry-run

# Trigger refinement in GitHub Actions (requires CLAUDE_CODE_OAUTH_TOKEN + REFINE_PAT secrets)
refine-remote *ARGS:
    gh workflow run refine.yml {{ARGS}}
    @echo "✓ Triggered refinement workflow"
    @echo "  Watch: gh run watch --workflow refine.yml"

# =============================================================================
# CI Performance Analysis
# =============================================================================

# Run CI performance report for a workflow run (defaults to latest on current branch)
ci-perf RUN_ID="":
    python3 scripts/ci-perf-report.py {{RUN_ID}}

# =============================================================================
# Utility Commands
# =============================================================================

# Remove testcontainers whose owner process has exited (safe for parallel test runs)
prune-testcontainers:
    #!/usr/bin/env bash
    set -euo pipefail
    STALE=""
    while IFS= read -r cid; do
        [ -z "$cid" ] && continue
        pid=$(docker inspect "$cid" --format '{{{{index .Config.Labels "tc-owner-pid"}}' 2>/dev/null || true)
        if [ -n "$pid" ] && ! kill -0 "$pid" 2>/dev/null; then
            STALE="$STALE $cid"
        fi
    done < <(docker ps -aq --filter "label=tc-owner-pid")
    if [ -n "$STALE" ]; then
        echo "Pruning orphaned testcontainers (dead owner PIDs)..."
        docker rm -f $STALE
    fi

# Clean build artifacts
clean: _clean-wasm
    cd service && cargo clean
    cd web && rm -rf node_modules/.cache dist .vite
    @echo "✓ Build artifacts cleaned"

# =============================================================================
# Documentation
# =============================================================================

# Build all documentation locally
docs: docs-rust docs-ts
    @echo "✓ All docs built"
    @echo "  Rust: target/doc/tinycongress_api/index.html"
    @echo "  TypeScript: web/docs/index.html"

# Build Rust API docs
docs-rust:
    cargo doc --workspace --no-deps

# Build TypeScript docs
docs-ts:
    cd web && yarn typedoc
    @echo "TypeScript docs: web/docs/index.html"

# =============================================================================
# Setup & Prerequisites
# =============================================================================

# Check prerequisites and display setup instructions
setup:
    @echo "=== TinyCongress Development Setup ==="
    @echo ""
    @just versions
    @echo ""
    @echo "Optional prerequisites for full-stack development:"
    @echo "  - Docker: $(docker --version 2>/dev/null || echo "NOT INSTALLED")"
    @echo "  - kubectl: $(kubectl version --client 2>/dev/null | head -1 || echo "NOT INSTALLED")"
    @echo ""
    @echo "Static analysis tools (optional, for lint-static):"
    @echo "  - typos: $(typos --version 2>/dev/null || echo "NOT INSTALLED - cargo install typos-cli")"
    @echo "  - hadolint: $(hadolint --version 2>/dev/null || echo 'NOT INSTALLED - brew install hadolint')"
    @echo "  - actionlint: $(actionlint --version 2>/dev/null || echo "NOT INSTALLED - brew install actionlint")"
    @echo "  - shellcheck: $(shellcheck --version 2>/dev/null | head -2 | tail -1 || echo "NOT INSTALLED - brew install shellcheck")"
    @echo ""
    @echo "For local development (no cluster needed):"
    @echo "  just lint          # Lint all code"
    @echo "  just fmt           # Format all code"
    @echo "  just build         # Build backend + frontend"
    @echo "  just test          # Run all unit tests"
    @echo "  just dev-backend   # Start backend with hot reload"
    @echo "  just dev-frontend  # Start Vite dev server"
    @echo ""
    @echo "For full-stack testing (requires Docker + KinD):"
    @echo "  just kind-create   # Create local KinD cluster (one-time)"
    @echo "  just test-ci       # Run full test suite via Skaffold"
    @echo "  just dev           # Start full-stack dev environment"
    @echo ""

# =============================================================================
# Database Commands
# =============================================================================

# Run database migrations (requires DATABASE_URL)
db-migrate:
    cd service && cargo run --bin tinycongress-api -- migrate

# =============================================================================
# SQLx Query Cache (Offline Mode)
# =============================================================================

# Regenerate SQLx query snapshots (requires DATABASE_URL)
sqlx-prepare:
    cd service && cargo sqlx prepare
    @echo "✓ SQLx snapshots regenerated in service/.sqlx/"

# Verify SQLx query snapshots are up-to-date (requires DATABASE_URL)
sqlx-check:
    cd service && cargo sqlx prepare --check
    @echo "✓ SQLx snapshots are up to date"

# =============================================================================
# Bot Development
# =============================================================================

# Port-forward LiteLLM and Exa cache proxies from the cluster
pf-bot:
    #!/usr/bin/env bash
    set -euo pipefail
    kubectl port-forward -n tiny-congress-demo svc/litellm 4001:4001 &
    kubectl port-forward -n tiny-congress-demo svc/exa-cache 4002:4002 &
    echo "LiteLLM: localhost:4001, Exa cache: localhost:4002"
    echo "Press Ctrl-C to stop"
    wait

# Enqueue a bot research task and show recent traces
bot-run company="Apple Inc." room_id="a1111111-1111-1111-1111-111111111111":
    #!/usr/bin/env bash
    set -euo pipefail
    echo "Enqueuing research_company task..."
    kubectl exec -n tiny-congress-demo deploy/tc-demo-postgres -- \
        psql -U postgres -d tiny_congress \
        -c "SELECT pgmq.send('rooms__bot_tasks', '{\"room_id\": \"{{room_id}}\", \"task\": \"research_company\", \"params\": {\"company\": \"{{company}}\"}}'::jsonb);"
    echo ""
    echo "Watching traces..."
    kubectl exec -n tiny-congress-demo deploy/tc-demo-postgres -- \
        psql -U postgres -d tiny_congress \
        -c "SELECT id, task, status, total_cost_usd, steps->0->>'output_summary' as first_step FROM rooms__bot_traces WHERE room_id = '{{room_id}}' ORDER BY created_at DESC LIMIT 5;"

# =============================================================================
# Info / Versions
# =============================================================================

# Show tool versions and check against required versions
versions:
    #!/usr/bin/env bash
    set -euo pipefail

    echo "Rust:     $(rustc --version | cut -d' ' -f2)"
    echo "Cargo:    $(cargo --version | cut -d' ' -f2)"

    # Node: check against web/.nvmrc
    NODE_REQ=$(cat web/.nvmrc)
    NODE_CUR=$(node --version | sed 's/v//' | cut -d. -f1)
    if [[ "$NODE_CUR" != "$NODE_REQ" ]]; then
        echo "Node:     v$NODE_CUR ⚠️  (requires $NODE_REQ, see web/.nvmrc)"
    else
        echo "Node:     $(node --version)"
    fi
    echo "Yarn:     $(yarn --version)"

    # Skaffold: check against .skaffold-version
    SKAFFOLD_REQ=$(cat .skaffold-version)
    if ! command -v skaffold &>/dev/null; then
        echo "Skaffold: NOT INSTALLED ⚠️  (requires v$SKAFFOLD_REQ)"
    else
        SKAFFOLD_CUR=$(skaffold version | sed 's/v//')
        if [[ "$SKAFFOLD_CUR" != "$SKAFFOLD_REQ" ]]; then
            echo "Skaffold: v$SKAFFOLD_CUR ⚠️  (requires v$SKAFFOLD_REQ)"
        else
            echo "Skaffold: v$SKAFFOLD_CUR"
        fi
    fi

    echo "Just:     $(just --version | cut -d' ' -f2)"
