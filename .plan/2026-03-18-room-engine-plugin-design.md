# Room Engine Plugin Architecture

**Date:** 2026-03-18
**Status:** Draft
**Goal:** Modularize rooms so that new interaction models (engines) can be authored independently of the core identity/trust platform, with a long-term path toward WASM-based community-authored room types.

## Problem

Every room in TinyCongress is a multi-dimensional slider poll. The only axis of variation is the eligibility constraint. To support fundamentally different interaction models — pairwise comparison, batch-synthesized reports, deliberation forums — the room concept needs a plugin boundary that separates **platform concerns** (identity, trust graph, eligibility, room lifecycle) from **engine concerns** (the interaction model, engine-specific data, engine-specific UI).

## Two-Layer Model

```
Platform (identity, trust, eligibility, room lifecycle index)
  └─ Room Engine (interaction model — trait/plugin boundary)
       └─ Room Instance (configured deployment of an engine)
```

- **Platform** owns identity, the trust graph, shared eligibility constraints, authentication, and the room lifecycle index (create/open/close/archive).
- **Room Engine** defines a fundamentally different interaction model. Each engine is a Rust crate implementing the `RoomEngine` trait. Engines own their database tables, API endpoints, and frontend components.
- **Room Instance** is a configured deployment of an engine. Each engine defines a config schema; instances store engine-specific configuration in a JSONB column on the platform's rooms table.

The current `service/src/rooms/` module is the **polling engine** — the first engine implementation.

## Architecture

### Backend: Crate Structure

```
crates/
  tc-engine-api/           # RoomEngine trait, platform types, shared constraints
  tc-engine-polling/       # First engine: multi-dimensional slider polls
  tc-engine-pairwise/      # Second engine: pairwise comparison / prioritization
```

All engine crates depend on `tc-engine-api`. The service crate depends on the engine crates and wires them into a registry at startup.

### The Engine Trait

```rust
// crates/tc-engine-api/src/lib.rs

pub trait RoomEngine: Send + Sync + 'static {
    /// Unique string identifier, e.g. "polling", "pairwise"
    fn engine_type(&self) -> &str;

    /// Human-readable metadata
    fn metadata(&self) -> EngineMetadata;

    /// Axum router fragment. Platform mounts under /rooms/{room_id}/...
    /// Routes receive EngineContext via Axum extension.
    /// PlatformState is the top-level Axum app state (contains EngineRegistry
    /// + EngineContext). Engines access EngineContext from it.
    fn routes(&self) -> axum::Router<PlatformState>;

    /// JSON schema describing engine-specific room configuration.
    /// Stored in rooms.engine_config JSONB column.
    fn config_schema(&self) -> serde_json::Value;

    /// Validate that a config blob is valid for this engine.
    fn validate_config(&self, config: &serde_json::Value) -> Result<(), ConfigError>;

    /// Called after a room instance is created. Initialize engine-specific state
    /// (match tables, bracket state, initial rounds, etc.).
    fn on_room_created(&self, room_id: Uuid, config: &serde_json::Value, ctx: &EngineContext) -> Result<()>;

    /// Spawn background tasks (lifecycle consumers, round managers, etc.)
    /// Called once at startup. Engine owns the join handles.
    fn start(&self, ctx: EngineContext) -> Result<Vec<tokio::task::JoinHandle<()>>>;
}
```

### Engine Context & Platform Services

Engines receive platform services via `EngineContext`, provided at startup and injected into routes as an Axum extension:

```rust
/// Everything an engine needs at runtime
pub struct EngineContext {
    pub pool: PgPool,
    pub trust_reader: Arc<dyn TrustGraphReader>,
    pub constraints: Arc<ConstraintRegistry>,
    pub room_lifecycle: Arc<dyn RoomLifecycle>,
}
```

```rust
/// Thin read-only trait over the trust graph.
/// Defined in tc-engine-api; implemented by the service crate
/// via delegation to the full TrustRepo.
///
/// Method signatures mirror actual constraint usage patterns:
/// - get_score() returns a composite snapshot (used by endorsed_by,
///   community, congress constraints)
/// - has_endorsement() takes a slice of verifier IDs (used by
///   identity_verified constraint)
pub trait TrustGraphReader: Send + Sync {
    /// Returns the composite trust score for a subject relative to an anchor.
    /// Contains trust_distance, path_diversity, and eigenvector_centrality.
    async fn get_score(&self, subject: Uuid, anchor: Option<Uuid>) -> Option<TrustScoreSnapshot>;

    /// Check if a subject has an active endorsement from any of the given
    /// verifiers for the specified topic.
    async fn has_endorsement(&self, subject: Uuid, topic: &str, verifier_ids: &[Uuid]) -> bool;
}

/// Composite trust score — mirrors the fields constraints actually use.
pub struct TrustScoreSnapshot {
    pub trust_distance: f64,
    pub path_diversity: u32,
    pub eigenvector_centrality: f64,
}

/// Room lifecycle operations — read room metadata, trigger lifecycle transitions.
pub trait RoomLifecycle: Send + Sync {
    async fn get_room(&self, room_id: Uuid) -> Result<RoomRecord>;
    async fn close_room(&self, room_id: Uuid) -> Result<()>;
}

/// Wraps the shared constraint implementations. Engines call check()
/// on submission routes to gate participation.
pub struct ConstraintRegistry {
    trust_reader: Arc<dyn TrustGraphReader>,
}

impl ConstraintRegistry {
    /// Evaluate the given constraint against a user.
    /// Returns Ok(Eligibility) with is_eligible + reason.
    pub async fn check(
        &self,
        constraint_type: &str,
        constraint_config: &serde_json::Value,
        user_id: Uuid,
    ) -> Result<Eligibility>;
}
```

`PlatformState` is the top-level Axum application state. It holds the `EngineRegistry` and the shared `EngineContext`. Engines access `EngineContext` from it:

```rust
/// Top-level Axum app state. Engines receive this as Router<PlatformState>.
pub struct PlatformState {
    pub engine_registry: Arc<EngineRegistry>,
    pub engine_ctx: EngineContext,
}
```

Authentication uses the existing `AuthenticatedDevice` Axum extractor — engines use it directly, no new abstraction needed.

### Shared Constraint Library

Eligibility constraints are a **shared library**, not engine-owned logic. The existing constraint implementations (`identity_verified`, `endorsed_by`, `community`, `congress`) move into `tc-engine-api`, depending on the `TrustGraphReader` trait (not the full `TrustRepo`). This breaks the dependency chain: `tc-engine-api` defines `TrustGraphReader` + constraints; the service crate implements `TrustGraphReader` by delegating to `TrustRepo`.

Engines select a constraint at room creation time but don't reimplement trust-graph gating. The `constraint_type` and `constraint_config` columns stay on the platform's `rooms__rooms` table.

**Eligibility is not platform-enforced middleware.** Engines call `constraints.check(constraint_type, config, user_id)` explicitly on submission routes. Read-only routes (results, rankings, metadata) are not gated.

Engines *may* layer additional eligibility checks on top (e.g. "must have participated in the previous round"), but the trust-graph gating is shared.

### Error Handling Contract

Engines return a standard `EngineError` type that maps to HTTP responses:

```rust
/// Standard engine error. Engines return this from handlers;
/// IntoResponse impl maps to the appropriate status code.
pub enum EngineError {
    NotFound(String),          // 404
    NotEligible(String),       // 403
    InvalidInput(String),      // 400
    Conflict(String),          // 409
    Internal(anyhow::Error),   // 500
}

impl IntoResponse for EngineError { /* status code mapping */ }
```

Engines can use `EngineError` directly or map their own error types into it via `From` impls.

### Platform Schema Changes

```sql
ALTER TABLE rooms__rooms
  ADD COLUMN engine_type TEXT NOT NULL DEFAULT 'polling',
  ADD COLUMN engine_config JSONB NOT NULL DEFAULT '{}';
```

Engine-specific tables use a namespaced prefix (`engine_polling__*`, `engine_pairwise__*`) and are managed as standard numbered migration files in `service/migrations/`, not via the trait. This preserves the existing migration ordering discipline.

**Known tradeoff:** `engine_config` as JSONB with runtime JSON Schema validation is stringly-typed, which conflicts with the "make wrong code hard to write" principle. This is an acceptable pragmatic choice for forward-compatibility with the WASM future, and consistent with the existing `constraint_config` pattern. While only compiled-in engines exist, each engine could internally deserialize into a typed config struct for compile-time safety. Revisit if this causes bugs.

### Route Structure

```
GET    /rooms                              → platform (list rooms)
POST   /rooms                              → platform (create, validates engine_type,
                                              delegates config validation to engine)
GET    /rooms/{room_id}                    → platform (metadata + engine_type)
PATCH  /rooms/{room_id}/status             → platform (lifecycle: open/close/archive)
GET    /rooms/{room_id}/eligibility        → platform (shared constraint check)
*      /rooms/{room_id}/*                  → delegated to engine (see below)

GET    /engines                            → platform (list registered engines + schemas)
```

**No `/engine/` prefix.** The platform router looks up the room's `engine_type`, then delegates the remaining path to the matching engine. The polling engine registers `/polls/...`, `/agenda`, etc. — same URLs as today, no breaking change.

The engine router middleware:
1. Looks up the room's `engine_type` from the database
2. Delegates to the matching engine's router
3. Injects `EngineContext` as an Axum extension

**Routing cost:** The `engine_type` lookup adds one DB query per room-scoped request. At current scale (pre-launch demo) this is negligible. If it becomes a concern, cache `room_id → engine_type` mappings in-memory with a short TTL — the mapping changes only when a room is created.

Collision guard: if two engines register the same sub-path segment, that's a startup panic. In practice this won't happen — engines own different interaction models with different resource names.

### Engine Registration

```rust
// service/src/engine_registry.rs
pub struct EngineRegistry {
    engines: HashMap<String, Arc<dyn RoomEngine>>,
}
```

Wired at startup in `main.rs`:

```rust
let mut registry = EngineRegistry::new();
registry.register(PollingEngine::new());
registry.register(PairwiseEngine::new());

// Start engine background tasks
let ctx = EngineContext { pool, trust_reader, constraints };
for engine in registry.all() {
    engine.start(ctx.clone())?;
}

let app = platform_router(registry.clone())
    .merge(engine_router(registry));
```

Intentionally simple: compiled-in, explicit registration. The WASM future replaces `registry.register(...)` with loading from a module store; the `RoomEngine` trait contract stays the same.

### Frontend Architecture

```
web/src/engines/
  api/
    types.ts              # Engine contract types
    hooks.ts              # Platform-provided hooks
    EngineRouter.tsx       # Dynamic dispatch component
  polling/
    components/           # SliderVote, PollResults, DimensionHistogram, etc.
    api/                  # client.ts, queries.ts
    PollEngineView.tsx    # Top-level engine component
    index.ts              # exports { EngineView, engineMeta }
  pairwise/
    components/           # PairCard, RankingResults, etc.
    api/
    PairwiseEngineView.tsx
    index.ts
  registry.ts             # engine_type string → lazy import
```

**Engine contract (TypeScript):**

```typescript
export interface EngineMeta {
  type: string;
  displayName: string;
  description: string;
}

export interface EngineViewProps {
  room: Room;           // platform Room record (includes engine_config)
  eligibility: Eligibility;  // from platform constraint check
}

// Each engine's index.ts exports:
// export const engineMeta: EngineMeta;
// export const EngineView: React.FC<EngineViewProps>;
```

**Platform-provided hooks:**

```typescript
export function useRoomLifecycle(roomId: string);  // open/close/status
export function useEligibility(roomId: string);     // shared constraint result
export function useAuth();                          // current user/device
```

**Dynamic dispatch:**

```typescript
const engineMap = {
  polling: () => import('./polling'),
  pairwise: () => import('./pairwise'),
};

export function EngineRouter({ room, eligibility }: EngineViewProps) {
  const Engine = React.lazy(engineMap[room.engine_type]);
  return <Engine.EngineView room={room} eligibility={eligibility} />;
}
```

**Key properties:**
- Lazy loading — engine code only loads when you visit a room of that type
- Zero build config changes — everything is inside `web/src/`, existing `@/` alias works
- Engine isolation — engines import from `@/engines/api/` for platform hooks, never from each other
- Adding an engine = create directory + add one line to `registry.ts`

## Boundary Summary

| Concern | Owner | Where |
|---|---|---|
| Identity, auth, device keys | Platform | `service/src/` (unchanged) |
| Trust graph | Platform | `service/src/trust/` (unchanged) |
| Eligibility constraints | Shared library | `ConstraintRegistry` in `crates/tc-engine-api/` (depends on `TrustGraphReader`, not `TrustRepo`) |
| Room CRUD, lifecycle index | Platform | `service/src/rooms/` (slimmed); exposed to engines via `RoomLifecycle` trait in `EngineContext` |
| Interaction model, engine-specific data | Engine | `crates/tc-engine-*/` |
| Engine-specific background tasks | Engine | Via `fn start()` on the trait |
| Engine-specific scheduling/queues | Engine | Engine-owned queue tables |
| Engine-specific UI | Engine | `web/src/engines/*/` |
| Engine registration & routing | Platform | `service/src/engine_registry.rs` |
| Error → HTTP mapping | Shared | `EngineError` in `crates/tc-engine-api/` |

## Migration Path

### Phase 1: Create `tc-engine-api`

Define the `RoomEngine` trait (including `on_room_created` lifecycle hook), `EngineContext`, `PlatformState`, `TrustGraphReader` trait, `TrustScoreSnapshot`, `RoomLifecycle` trait, `ConstraintRegistry`, `EngineError`, and engine metadata types.

**Key dependency work:** Extract `TrustGraphReader` as a thin read-only interface with `get_score()` (returning `TrustScoreSnapshot`) and `has_endorsement()` (taking `&[Uuid]` for verifier IDs) — matching the actual usage patterns of the existing constraints. The constraint implementations (`identity_verified`, `endorsed_by`, `community`, `congress`) are rewritten to depend on `TrustGraphReader` instead of `TrustRepo`, wrapped in a `ConstraintRegistry`. The service crate implements `TrustGraphReader` by delegating to its existing `TrustRepo`.

### Phase 2: Extract polling engine

Move poll-specific code from `service/src/rooms/` into `crates/tc-engine-polling/`:
- `repo/polls.rs`, `repo/votes.rs`, `repo/lifecycle_queue.rs` → engine repo
- `service.rs` (poll logic) → engine service
- `lifecycle.rs` (auto-cadence consumer) → engine lifecycle, spawned via `fn start()`
- `http/` (poll-specific routes) → engine http

Room CRUD and the rooms table stay in `service/src/rooms/` as platform code. Add `engine_type` and `engine_config` columns via a standard numbered migration.

**Key sub-step:** The current `RoomsService` trait bundles room CRUD and all poll/vote operations into a single trait. This must be split: room CRUD stays in the platform's `RoomsService`, poll/vote/lifecycle operations move to the polling engine's internal service layer. This split touches every test and caller of the current trait — it is the most labor-intensive part of this phase.

The polling engine's lifecycle queue (`rooms__lifecycle_queue`) becomes engine-owned. The existing check constraint on `message_type` stays as-is — it's specific to the polling engine and other engines will have their own queue tables if needed.

### Phase 3: Refactor frontend

Move poll-specific UI from `web/src/features/rooms/` into `web/src/engines/polling/`:
- `api/`, `components/`, `hooks/` → engine directory
- `Poll.page.tsx` guts → `PollEngineView.tsx`
- `Poll.page.tsx` shell → thin wrapper rendering `EngineRouter`
- `Rooms.page.tsx` → stays as platform room list, adds engine type display

Create `web/src/engines/api/` with the TS contract and platform hooks.

### Phase 4: Prove it works

All existing tests pass. Zero behavior change. This is a pure refactor — the polling engine does exactly what `service/src/rooms/` does today, just behind the `RoomEngine` trait. URLs are unchanged.

### Phase 5: Build pairwise engine

The payoff: author `tc-engine-pairwise` and `web/src/engines/pairwise/` against the established contracts. Port the Go interface definitions from the existing room design spec (pairwise comparison with Bradley-Terry scoring, active bandit sampling, per-tick caps). This is the "vibe code a new engine" experience.

## Future: WASM Path

The `RoomEngine` trait is the stable interface. The WASM migration path:
1. Today: engines are Rust crates, compiled in, registered explicitly
2. Future: engines compile to WASM modules, loaded from a module store at runtime
3. The trait contract doesn't change — WASM modules implement the same interface via a host-guest binding layer (e.g. `wasmtime` component model)

WASM applies at the **Room Instance** layer first (configurable rulesets within an engine) before it applies at the engine layer itself. An engine like polling could accept WASM modules that define custom aggregation functions, scoring rules, or eligibility extensions — without replacing the engine's core interaction model.

## Design Validation

The pairwise comparison engine is the validation target. It should be buildable by:
1. Reading `crates/tc-engine-api/` for the Rust trait
2. Reading `web/src/engines/api/` for the TS interface
3. Creating `crates/tc-engine-pairwise/` and `web/src/engines/pairwise/`
4. Never touching platform code

If that workflow works, the abstraction is right. If it requires reaching into platform internals, the trait is missing something.
