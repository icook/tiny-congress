# ADR-009: Simulation Worker as Pure HTTP API Client

## Status
Accepted

## Context
The demo environment needs synthetic content (rooms, polls, votes) to showcase TinyCongress. The original "seed" module accessed the database directly — it shared the `PgPool` and ran SQL inserts to populate rooms and cast votes.

This approach had problems:
- Bypassed all HTTP-layer validation, auth middleware, and business logic
- Created a coupling between the seeder and the database schema — migration changes broke the seeder
- Couldn't verify that the API actually worked end-to-end for the operations it was seeding
- Violated the trust boundary: the seeder handled key material server-side to create accounts, which real clients never do

## Decision
The simulation worker (`sim` binary) is a pure HTTP API client. It interacts exclusively through the public REST API — the same endpoints real users and frontends hit. It has no database dependency and no access to `PgPool`.

**Identity:** The sim derives deterministic Ed25519 key pairs from seed indices via `SHA-256("tc-sim-{key-type}-v1-{index}")`. The same index always produces the same keys, making runs reproducible and idempotent.

**Account lifecycle:** `POST /auth/signup` (201 = new, 409 = exists, proceed). No login needed for voter accounts — device-key signing is per-request using the key registered at signup.

**Verifier:** A separate deterministic identity (`SimAccount::verifier()`) logs in via `POST /auth/login` to register a device key, then signs `POST /verifiers/endorsements` requests to grant voting eligibility.

**Content:** LLM-generated room topics via OpenRouter, inserted through `POST /rooms`, `POST /rooms/{id}/polls`, etc.

**Votes:** Discovered via API (`GET /rooms` → `GET /rooms/{id}/polls` → `GET /rooms/{id}/polls/{id}/results`), cast through `POST /rooms/{id}/polls/{id}/vote`.

## Consequences

### Positive
- Dogfoods the API — every sim run is an implicit integration test
- Enforces the trust boundary: all crypto happens client-side, matching the real user flow
- No database coupling — the sim works against any TinyCongress API instance
- Can run from anywhere (laptop, CronJob, CI) with just a URL and API keys
- Idempotent: re-running with the same config produces the same accounts and skips existing content (409/upsert semantics)

### Negative
- More API calls than direct DB inserts (O(rooms × polls) for discovery, one call per vote)
- Depends on the API being up — can't seed an empty database without a running server
- Error diagnosis is harder: failures surface as HTTP status codes, not SQL errors

### Neutral
- The `seed` module and binary were deleted entirely — no migration path, clean replacement
- The sim binary ships in the same Docker image as the API (`COPY --from=builder .../sim`)
- Configuration uses `SIM_*` env vars (not `TC_*`) to avoid collision with the API config

## Alternatives considered

### Direct database access (original approach)
- Seeder shared `PgPool` with the API, ran SQL inserts directly
- Rejected: bypassed validation and auth, created schema coupling, violated trust boundary, didn't verify the API worked

### Hybrid approach (DB for accounts, API for content)
- Create accounts directly in the database, but use the API for rooms/polls/votes
- Rejected: still violates the trust boundary for account creation (server-side key handling), and the partial approach is harder to reason about than a clean boundary

### GraphQL instead of REST
- Use the GraphQL API for content operations
- Rejected: the REST API covers all needed operations, GraphQL adds complexity (query construction, fragment management) without benefit for a machine client

## References
- PR #380: Sim module implementation
- `service/src/sim/`: Module source
- `service/src/bin/sim.rs`: Orchestration entry point
- `docs/interfaces/environment-variables.md`: `SIM_*` configuration
- ADR-008: Account-based verifiers (the sim's endorsement mechanism)
