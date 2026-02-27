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
test-backend: _ensure-test-postgres
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

# Export GraphQL schema from Rust backend
export-schema:
    cd service && cargo run --bin export_schema > ../web/schema.graphql

# Export OpenAPI schema from Rust backend
export-openapi:
    cd service && cargo run --bin export_openapi > ../web/openapi.json

# Generate TypeScript types and Zod schemas from GraphQL schema
codegen-graphql:
    cd web && yarn graphql-codegen

# Generate TypeScript types from OpenAPI schema
codegen-openapi:
    cd web && yarn openapi-typescript openapi.json -o src/api/generated/rest.ts && yarn prettier --write src/api/generated/rest.ts

# Full codegen: export schemas from Rust + generate TypeScript types
codegen: export-schema export-openapi codegen-graphql codegen-openapi
    @echo "✓ GraphQL and REST types generated"

# =============================================================================
# Frontend (React/TypeScript) Commands
# =============================================================================

# Run frontend unit tests (requires WASM artifacts)
test-frontend: _build-wasm-dev
    cd web && yarn vitest

# Run frontend unit tests in watch mode (requires WASM artifacts)
test-frontend-watch: _build-wasm-dev
    cd web && yarn vitest:watch

# Run full frontend test suite (typecheck + lint + vitest + build) - includes E2E via CI
test-frontend-full: _build-wasm-dev
    cd web && yarn test

# Run frontend E2E tests with Playwright (requires running backend/frontend)
test-frontend-e2e:
    cd web && yarn playwright:ci

# Internal: Check frontend types
_typecheck-frontend:
    cd web && yarn typecheck

# Run all frontend linting (prettier + eslint + stylelint)
lint-frontend:
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
build-frontend:
    cd web && yarn build

# Run frontend dev server
dev-frontend:
    cd web && yarn dev

# Run Storybook dev server
dev-storybook:
    cd web && yarn storybook

# Build Storybook static site
build-storybook:
    cd web && yarn storybook:build

# Install frontend dependencies
install-frontend:
    cd web && yarn install

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

# Start full development environment with Skaffold (hot reload, port forwarding)
# Prerequisites: Docker, Skaffold, KinD cluster (just kind-create)
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

# Deploy to local cluster
deploy:
    @echo "Deploying to Kubernetes cluster..."
    skaffold run -p dev

# Deploy release images to cluster
deploy-release:
    @echo "Deploying release images to cluster..."
    skaffold run -p release

# Delete all deployed resources from cluster
undeploy:
    @echo "Cleaning up Kubernetes resources..."
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
lint: lint-backend lint-frontend
    @echo "✓ All linting passed"

# Fix all formatting (backend + frontend)
fmt: fmt-backend fmt-frontend
    @echo "✓ All formatting applied"

# Type check frontend
typecheck: _typecheck-frontend

# Run all local unit tests (backend + frontend + wasm)
test: test-backend test-wasm test-frontend
    @echo "✓ Unit tests passed"

# Run frontend unit tests with coverage
test-frontend-cov: _build-wasm-dev
    cd web && yarn vitest:coverage

# Run all unit tests with coverage
test-cov: test-backend-cov test-frontend-cov
    @echo "✓ Coverage reports generated"

# Build everything locally (no images)
build: build-backend build-wasm build-frontend
    @echo "✓ All builds successful"

# Build everything in release mode
build-release: build-backend-release build-frontend
    @echo "✓ Release builds successful"

# =============================================================================
# Full-Stack Testing (Requires Docker + Kubernetes)
# =============================================================================

# Run full test suite via Skaffold (mirrors CI - RECOMMENDED per AGENTS.md)
test-ci: _build-images-artifacts
    @echo "Running full test suite via Skaffold..."
    skaffold test --build-artifacts artifacts.json

# =============================================================================
# Utility Commands
# =============================================================================

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

# Build and open Rust API docs
docs-rust:
    cargo doc --workspace --no-deps --open

# Build TypeScript docs
docs-ts:
    cd web && yarn typedoc
    @echo "TypeScript docs: web/docs/index.html"

# =============================================================================
# Git Workflows
# =============================================================================
# See docs/interfaces/branch-naming-conventions.md for branch naming standards

# Push current branch and create a PR
pr title body="":
    #!/usr/bin/env bash
    set -euo pipefail
    branch=$(git rev-parse --abbrev-ref HEAD)
    echo "→ Pushing branch: $branch"
    git push -u origin "$branch"
    echo "→ Creating PR..."
    gh pr create --title "{{title}}" --body "{{body}}"
    pr_num=$(gh pr view --json number -q .number)
    echo "✓ PR #$pr_num created: $(gh pr view --json url -q .url)"

# Push current branch, create PR, and enable auto-merge
pr-auto title body="":
    #!/usr/bin/env bash
    set -euo pipefail
    branch=$(git rev-parse --abbrev-ref HEAD)
    echo "→ Pushing branch: $branch"
    git push -u origin "$branch"
    echo "→ Creating PR..."
    gh pr create --title "{{title}}" --body "{{body}}"
    pr_num=$(gh pr view --json number -q .number)
    echo "→ Enabling auto-merge for PR #$pr_num..."
    gh pr merge "$pr_num" --auto --merge
    echo "✓ PR #$pr_num created with auto-merge enabled: $(gh pr view --json url -q .url)"

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
