# TinyCongress

This is a non-functional WIP monorepo for a web community.

## Core Components

- `/web/`
    - A simple React UI will offer allow users to create accounts, manage keys and participate in polling rooms. Mantine UI has been picked for the component library.
- `/service/`
    - A Rust based graphql API implements polling room runtime and CRUD endpoints. axum, tokio, and sqlx.

# Dev

[Skaffold](https://skaffold.dev/) manages:

- Local dev cluster with hot reload
- CI cluster setup test running
- Production image building

# macOS Developer Setup

This guide gets you from a clean macOS to running TinyCongress locally.

## Prerequisites

- macOS 13+ (Apple Silicon or Intel)
- Admin user

## Install Homebrew

```bash
/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
# Follow on-screen PATH instructions (usually add to ~/.profile)
```

## Core tooling

```bash
brew install git gnupg jq shellcheck
brew install --cask docker
brew install kubectl minikube skaffold
```

- Open Docker Desktop once to finalize installation.

## Start dev cluster

```bash
minikube start
skaffold dev --port-forward
```

## Build metadata and smoke tests

- The API exposes build details at `buildInfo { version gitSha buildTime message }` via the GraphQL endpoint.
- Run `make e2e-smoke` to deploy with Skaffold, port-forward the UI (`4173`) and API (`8080`), and execute the Playwright `@smoke` check against `/about`. Set `SKAFFOLD_PROFILE` or `SKAFFOLD_ARTIFACTS_FILE` to reuse existing builds when needed.
