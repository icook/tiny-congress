# TinyCongress Development Toolchain
# Recipes aligned with AGENTS.md guidelines
#
# Quick Start:
#   1. just setup           # Check prerequisites (one-time setup)
#   2. just dev             # Start full-stack dev environment (requires Skaffold + cluster)
#
# Local Development (no cluster needed):
#   - just lint             # Lint all code
#   - just fmt              # Format all code
#   - just test-backend     # Run backend unit tests
#   - just build-backend    # Build backend
#   - just dev-backend      # Start backend with hot reload
#   - just dev-frontend     # Start Vite frontend dev server
#
# Full-Stack Testing (requires Docker + Kubernetes):
#   - just test-full        # Build images, run all tests via Skaffold (mirrors CI)
#   - just test-ci          # Alias for test-full
#
# Run `just --list` for complete recipe list

# Default recipe - show help
default:
    @just --list

# =============================================================================
# Backend (Rust) Commands
# =============================================================================

# Run backend unit tests
test-backend:
    cd service && cargo test --test api_tests --test graphql_tests --test model_tests

# Run backend unit tests with coverage
test-backend-cov:
    cd service && cargo llvm-cov --test api_tests --test graphql_tests --test model_tests

# Run backend integration tests via Skaffold (RECOMMENDED - requires Docker + Kubernetes)
test-backend-integration:
    @echo "Building images and running integration tests via Skaffold..."
    skaffold build --file-output artifacts.json && skaffold test --build-artifacts artifacts.json

# Verify full test suite via Skaffold (CI mode - RECOMMENDED approach per AGENTS.md)
verify-ci:
    @echo "Running full CI verification (Skaffold, unit tests, integration tests, E2E)..."
    skaffold verify -p ci

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
# Code Generation (GraphQL Types)
# =============================================================================

# Export GraphQL schema from Rust backend
export-schema:
    cd service && cargo run --bin export_schema > ../web/schema.graphql

# Generate TypeScript types and Zod schemas from GraphQL schema
codegen-frontend:
    cd web && yarn graphql-codegen

# Full codegen: export schema from Rust + generate TypeScript/Zod
codegen: export-schema codegen-frontend
    @echo "✓ GraphQL types generated"

# =============================================================================
# Frontend (React/TypeScript) Commands
# =============================================================================

# Run frontend unit tests (if they exist)
test-frontend:
    cd web && yarn vitest

# Run frontend unit tests in watch mode (if they exist)
test-frontend-watch:
    cd web && yarn vitest:watch

# Run full frontend test suite (typecheck + lint + vitest + build) - includes E2E via CI
test-frontend-full:
    cd web && yarn test

# Run frontend E2E tests with Playwright (requires running backend/frontend)
test-frontend-e2e:
    cd web && yarn playwright:ci

# Check frontend types
typecheck-frontend:
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

# Run Storybook dev server (experimental - may have dependency issues)
storybook:
    cd web && yarn storybook

# Install frontend dependencies
install-frontend:
    cd web && yarn install

# =============================================================================
# Full-Stack Development (Requires Skaffold + Kubernetes Cluster)
# =============================================================================

# Start full development environment with Skaffold (hot reload, port forwarding)
# Prerequisites: Docker, Skaffold, Kubernetes cluster (minikube/Docker Desktop)
dev:
    @echo "Starting full-stack dev with Skaffold..."
    @echo "Prerequisites: ensure your Kubernetes cluster is running (e.g., minikube start)"
    skaffold dev -p dev --port-forward

# Build all container images (for current profile)
build-images:
    @echo "Building container images..."
    skaffold build

# Build images and output artifacts JSON (for reuse with test)
build-images-artifacts:
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
# Quality Checks (Local - No Cluster Required)
# =============================================================================

# Run all linting (backend + frontend) - no cluster required
lint: lint-backend lint-frontend
    @echo "✓ All linting passed"

# Fix all formatting (backend + frontend)
fmt: fmt-backend fmt-frontend
    @echo "✓ All formatting applied"

# Type check frontend
typecheck: typecheck-frontend

# Run all local unit tests (backend + frontend, if they exist)
test: test-backend test-frontend
    @echo "✓ Unit tests passed"

# Run all unit tests with coverage
test-cov: test-backend-cov test-frontend
    @echo "✓ Coverage reports generated"

# Build everything locally (no images)
build: build-backend build-frontend
    @echo "✓ All builds successful"

# Build everything in release mode
build-release: build-backend-release build-frontend
    @echo "✓ Release builds successful"

# =============================================================================
# Full-Stack Testing (Requires Docker + Kubernetes)
# =============================================================================

# Run full test suite via Skaffold (mirrors CI - RECOMMENDED per AGENTS.md)
test-full: build-images-artifacts
    @echo "Running full test suite via Skaffold..."
    skaffold test --build-artifacts artifacts.json

# Alias for test-full (CI-friendly naming)
test-ci: test-full

# =============================================================================
# Utility Commands
# =============================================================================

# Clean build artifacts
clean:
    cd service && cargo clean
    cd web && rm -rf node_modules/.cache dist .vite
    @echo "✓ Build artifacts cleaned"

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
    @echo "  just node-use      # Switch to correct Node version (requires nvm)"
    @echo "  just lint          # Lint all code"
    @echo "  just fmt           # Format all code"
    @echo "  just build         # Build backend + frontend"
    @echo "  just test-backend  # Run backend unit tests"
    @echo "  just dev-backend   # Start backend with hot reload"
    @echo "  just dev-frontend  # Start Vite dev server"
    @echo ""
    @echo "For full-stack testing (requires Docker + Kubernetes):"
    @echo "  minikube start     # Start local Kubernetes cluster"
    @echo "  just test-ci       # Run full test suite via Skaffold"
    @echo "  just dev           # Start full-stack dev environment"
    @echo ""

# Switch to Node version from .nvmrc (requires nvm)
node-use:
    @echo "Switching to Node version from web/.nvmrc..."
    @echo "Run: cd web && nvm use"
    @echo ""
    @echo "Or add this to your shell profile for automatic switching:"
    @echo '  # Auto-switch node version when entering directory with .nvmrc'
    @echo '  autoload -U add-zsh-hook'
    @echo '  load-nvmrc() { [[ -f .nvmrc ]] && nvm use; }'
    @echo '  add-zsh-hook chpwd load-nvmrc'

# =============================================================================
# Database Commands
# =============================================================================

# Run database migrations (requires DATABASE_URL)
db-migrate:
    cd service && cargo run --bin tinycongress-api -- migrate

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
