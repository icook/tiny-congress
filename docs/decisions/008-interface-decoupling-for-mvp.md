# ADR-008: Interface decoupling for MVP (modular monolith)

## Status
Accepted

## Context
Three components are intended to eventually be operable by third parties. The question is how much to decouple them for the MVP, given the primary goal is proving the core opinion-collection loop before investing in distributed infrastructure.

Four options were considered along a spectrum from zero isolation to full microservices.

## Decision
**Option 2: Interface decoupling with shared data.** Ship a single deployable binary backed by one database, but enforce strict compile-time boundaries between components so that later extraction over HTTP or protobuf is mechanical rather than a rewrite.

This means a modular monolith with hard boundaries: workspace crates per component, owned data per component, and explicit versioned interfaces (traits + DTOs) for all cross-component interaction.

### Why Option 2
- One deployable and one operational surface for MVP.
- Clear seams for third-party operation later.
- Extraction path without rewriting core logic.
- Aligns with the scaling notes: a single-machine deployment can go very far for the realtime voting core.

### Guardrails to prevent abstraction bleed

**Rule A: Owned data, shared database.** Component A is the only writer of A's tables. Other components call A's interface, never its tables. Violating this collapses you back to Option 1.

**Rule B: No shared domain structs across components.** Share only IDs, boundary DTOs, and versioned schema types. Everything else stays private.

**Rule C: Explicit ports and adapters from day 1.** Every cross-component call goes through a trait ("port") and an implementation ("adapter"). In MVP the adapter is in-process. Later, swap it for HTTP/gRPC without touching business logic.

**Rule D: Version the boundary now.** `v1` module namespace in each `*_api` crate. Backward-compatible changes only. This is what makes "operable by third parties" real even before extraction.

### Crate layout

For each component X:
- `x_api` - public DTOs, IDs, service traits. Minimal deps. Must compile fast and stay stable.
- `x_core` - domain logic. Implements `x_api` traits. Internal models not exported.
- `x_persist` - SQL, migrations, row mapping. Owns schema. Not depended on by other components.
- `app` - binary crate that wires components together, owns process setup, config, HTTP server.

**Dependency rules (compile-time enforced):**
- `x_core` may depend on `x_api` and shared utility crates.
- `x_persist` may depend on `x_api` (and optionally `x_core`), plus DB libs.
- `x_api` must not depend on any other component crate, DB crates, or web frameworks.
- Component A must not depend on Component B's `*_core` or `*_persist`. Cross-component use is only via `b_api`.

### Data isolation (shared DB)
- Every table has a single owning component. Only the owner writes to or defines schema for its tables.
- Migrations partitioned by component (folder or naming convention) with owner tags.
- Cross-owner foreign keys avoided; reference by stable IDs without FK constraints in MVP.
- Cross-component data exchange uses stable IDs and boundary DTOs, never DB rows.

### Interface standards
- Each component exposes commands (mutate), queries (read), and events (facts emitted).
- DTOs live in `*_api::v1`, are `#[non_exhaustive]`, and derive `Serialize`/`Deserialize`.
- Error enums in `*_api::v1` with stable kinds: `InvalidInput`, `NotFound`, `Conflict`, `Unauthorized`, `Internal`.
- Components must not expose DB handles, raw SQL, framework types, or internal state machine structs.

### Upgrade path from 2 to 4
When third-party demand is real:
1. Keep interfaces stable.
2. Add an outbox table for emitted events.
3. Replace in-proc adapter with RPC adapter.
4. Give the extracted service its own database.
5. Feed via events.
6. Delete old tables after cutover.

This avoids ever passing through Option 3.

## Consequences

### Positive
- Single deployable keeps MVP operationally simple
- Compile-time enforcement catches boundary violations before they ship
- Extraction later is mechanical: swap adapter, move crate, add network transport
- Third-party operability is a future swap, not a future rewrite
- DTOs are serializable from day 1, so nothing blocks adding network transport

### Negative
- More crates and more boilerplate than a flat module structure
- Cross-component queries require going through owned-component interfaces (no cross-joins)
- Developers must understand the boundary rules to avoid accidental coupling
- Some duplication of types at boundaries (DTOs vs internal domain models)

### Neutral
- Same runtime behavior as a monolith; boundaries are compile-time only
- Migration tooling unchanged; just organized by ownership
- Testing strategy adds contract tests but doesn't change existing test infra

## Alternatives considered

### Option 1: No decoupling
- Fastest to ship. Highest risk of "can't extract later" because call sites and data access sprawl unchecked.
- Rejected: even if you pick 1, you must act like 2 internally or pay later. Better to commit to the boundaries explicitly.

### Option 3: Separate services with naive syncing
- Worst of both worlds. Pay microservice tax now, still rewrite syncing later, debug distributed partial failure while product is unproven.
- Rejected: pure overhead for an MVP.

### Option 4: Separate services with robust syncing
- Only justified if multi-operator separation is required on day 1, or hard security/compliance isolation is needed.
- Rejected: scope explosion that competes with proving the core opinion-collection loop. Revisit when third-party demand is real.

## References
- Related: ADR-001 (cargo-chef Docker builds) for single-binary deployment
- `docs/` for component documentation standards
