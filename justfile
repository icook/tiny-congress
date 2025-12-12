# TinyCongress Development Toolchain
# Run `just --list` to see all available recipes

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

# Run backend integration tests (requires running database)
test-backend-integration:
    cd service && cargo test --test integration_tests

# Run backend integration tests with coverage
test-backend-integration-cov:
    cd service && cargo llvm-cov --test integration_tests

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
# Frontend (React/TypeScript) Commands
# =============================================================================

# Run frontend unit tests
test-frontend:
    cd web && yarn vitest run

# Run frontend unit tests in watch mode
test-frontend-watch:
    cd web && yarn vitest

# Run frontend E2E tests with Playwright
test-frontend-e2e:
    cd web && yarn playwright:ci

# Run full frontend test suite (typecheck + lint + vitest + build)
test-frontend-full:
    cd web && yarn test

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

# Run Storybook dev server
storybook:
    cd web && yarn storybook

# Build Storybook
storybook-build:
    cd web && yarn storybook:build

# Install frontend dependencies
install-frontend:
    cd web && yarn install

# =============================================================================
# Skaffold / Kubernetes Commands
# =============================================================================

# Start full development environment with Skaffold
dev:
    skaffold dev -p dev --port-forward

# Build all container images
build-images:
    skaffold build

# Build images and output artifacts JSON
build-images-artifacts:
    skaffold build --file-output artifacts.json

# Run Skaffold tests
skaffold-test:
    skaffold test

# Deploy to local cluster
deploy:
    skaffold run -p dev

# Deploy release images
deploy-release:
    skaffold run -p release

# Delete deployed resources
undeploy:
    skaffold delete

# =============================================================================
# Combined / Convenience Commands
# =============================================================================

# Run all linting (backend + frontend)
lint: lint-backend lint-frontend

# Fix all formatting (backend + frontend)
fmt: fmt-backend fmt-frontend

# Run all unit tests (backend + frontend)
test: test-backend test-frontend

# Run all tests with coverage
test-cov: test-backend-cov test-frontend

# Build everything
build: build-backend build-frontend

# Build everything (release)
build-release: build-backend-release build-frontend

# Run CI checks locally (lint + test + build)
ci: lint test build

# Clean build artifacts
clean:
    cd service && cargo clean
    cd web && rm -rf node_modules/.cache dist .vite

# =============================================================================
# Database Commands
# =============================================================================

# Run database migrations (requires DATABASE_URL)
db-migrate:
    cd service && cargo run --bin tinycongress-api -- migrate

# =============================================================================
# Help / Info Commands
# =============================================================================

# Show Rust version
rust-version:
    rustc --version
    cargo --version

# Show Node version
node-version:
    node --version
    yarn --version

# Show all tool versions
versions: rust-version node-version
    @echo ""
    @just --version
