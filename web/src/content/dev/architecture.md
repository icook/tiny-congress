## System Components

TinyCongress has four major pieces: a React frontend, a Rust API server, a
background trust engine, and a shared crypto library that runs on both sides.

### Frontend (Browser)

Built with React, Vite, Mantine, and TanStack Router/Query.

| Component | Role |
|-----------|------|
| UI Components | Mantine-based pages and forms |
| CryptoProvider | Lazily initializes tc-crypto WASM before child components render |
| DeviceProvider | Manages device key storage in IndexedDB |
| signedFetchJson | Signs REST requests using SubtleCrypto Ed25519 |
| @noble/curves | Ed25519 key pair generation |

The browser is the **trust boundary** — key generation, signing, and backup
encryption all happen here. The server never sees private key material.

### API Server (tinycongress-api)

A Rust binary built on Axum. Requests flow through three layers:

```
HTTP handler (thin adapter: deserialize, call service, map errors)
    ↓
Service (validation, orchestration, business rules)
    ↓
Repo (PostgreSQL via sqlx, trait-based for testability)
```

Four domain modules follow this pattern:

| Module | Routes | Purpose |
|--------|--------|---------|
| identity | `/auth/*` | Signup, login, device management, backup envelopes |
| reputation | `/me/endorsements`, `/verifiers/*` | Endorsements, ID.me OAuth verification |
| rooms | `/rooms/*`, `/polls/*`, `/vote` | Rooms, polls, dimensions, voting |
| trust | `/trust/endorse`, `/revoke`, `/scores` | Trust graph actions and scores |

Authentication uses Ed25519 request signing (not bearer tokens). The
`AuthenticatedDevice` extractor verifies signatures using tc-crypto natively.

### Background Workers

Running as tokio tasks inside the same API process:

- **TrustWorker** — polls the `trust_action_queue` table, processes endorsements
  and revocations, triggers score recomputation
- **TrustEngine** — walks the endorsement graph via a recursive SQL CTE to
  compute trust scores and path diversity
- **Nonce Cleanup** — deletes expired request nonces every 60 seconds

Trust actions are **asynchronous** — endpoints like `/trust/endorse` return
`202 Accepted` and enqueue work for the background worker.

### Shared Crypto (tc-crypto)

A single Rust crate compiled two ways:

| Target | Used for |
|--------|----------|
| Native (backend) | Ed25519 signature verification, KID derivation, envelope parsing |
| WASM (frontend) | KID derivation, base64url encoding/decoding |

This guarantees both sides produce identical results for shared operations.

### External Integrations

- **ID.me OAuth 2.0** — identity verification for voting eligibility
- **OpenRouter LLM** — used only by the `tc-sim` binary to generate demo
  rooms and polls (not part of the main API)

### Standalone Binaries

| Binary | Purpose |
|--------|---------|
| `tc-sim` | Simulation worker — creates rooms/polls via the REST API |
| `demo_verifier` | Bootstrap tool for demo environments |
| `export_openapi` | Dumps OpenAPI spec from the running server |
| `export_schema` | Dumps GraphQL schema |

### Database

PostgreSQL via sqlx with compile-time checked queries. Migrations live in
`service/migrations/`. Key tables span the four domain modules: accounts,
device keys, backup envelopes, endorsements, rooms, polls, votes, trust
actions, and trust scores.
