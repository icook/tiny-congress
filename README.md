# TinyCongress

A community governance platform where verified people vote on issues that matter — with more nuance than yes/no.

Users generate Ed25519 key pairs client-side; the server never sees private keys. Multi-dimensional polls let communities express the *shape* of their opinion, not just a binary position.

**Demo**: [demo.tinycongress.com](https://demo.tinycongress.com) | **Website**: [tinycongress.com](https://tinycongress.com)

## How It Works

1. **Sign up** — create an account with a username and backup password. Keys are generated in your browser.
2. **Enter a room** — browse open rooms with active polls on topics that matter to your community.
3. **Vote on dimensions** — instead of yes/no, rate each dimension (importance, urgency, feasibility) on a slider.
4. **See results** — watch the collective picture emerge as more people vote.

## Architecture

| Component | Stack | Purpose |
|-----------|-------|---------|
| `web/` | React, Mantine, Vite, TanStack Router | Client-side UI and crypto (via `tc-crypto` WASM) |
| `service/` | Rust, axum, sqlx, PostgreSQL | API, polling runtime, identity management |
| `crates/tc-crypto/` | Rust (native + WASM) | Shared cryptographic operations |
| `kube/` | Helm, Skaffold, KinD | Kubernetes deployment and CI |

**Trust model**: the server is a dumb witness, not a trusted authority. Crypto operations happen in the browser. See [docs/domain-model.md](docs/domain-model.md) for details.

## Development

```bash
# Run all tests (no cluster required)
just test

# Run linting
just lint

# Full CI suite (requires KinD cluster)
just test-ci

# Frontend dev server only
just dev-frontend
```

Run `just --list` for all available commands. See [docs/README.md](docs/README.md) for playbooks, interfaces, ADRs, and full documentation.

## Status

Pre-launch. Building toward a friends-and-family demo. Core flow (signup, rooms, voting, results) is functional. See `objectives.md` for the current readiness checklist.
# Burn test run 1
# Burn test run 2
# Burn test run 3
