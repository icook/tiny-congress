# Trust Graph Simulation Harness — Implementation Plan

**Issue:** #624
**Branch:** `test/624-trust-simulation-harness`
**Status:** Ready for implementation
**Depends on:** Nothing — trust anchor bootstrap landed on master

## Goal

Build a simulation test harness that constructs adversarial graph topologies and runs the real `TrustEngine` against them. Validates Sybil resistance claims empirically. Developer tool — not user-facing.

## Architecture

**Location:** `service/tests/common/simulation/`

### Components

#### `GraphBuilder` (`mod.rs`)

Core abstraction for constructing test topologies programmatically.

```rust
pub enum Team { Blue, Red }

pub struct SimNode {
    pub id: Uuid,
    pub name: String,
    pub team: Team,
}

pub struct GraphBuilder {
    pool: PgPool,
    nodes: Vec<SimNode>,
    edges: Vec<(Uuid, Uuid, f32)>,  // (endorser, subject, weight)
}
```

**Methods:**
- `new(pool) -> Self` — takes a pool from `isolated_db()`
- `add_node(name, team) -> Uuid` — creates account via `AccountFactory`, stores `SimNode`
- `endorse(from, to, weight)` — inserts endorsement row directly
- `endorse_revoked(from, to, weight)` — inserts revoked endorsement
- `node(name) -> Uuid` — lookup by name (panics if missing)
- `nodes_by_team(team) -> Vec<Uuid>` — filter by team designation

#### `TopologyGenerator` (`topology.rs`)

Parameterized constructors that call `GraphBuilder` methods.

- `hub_and_spoke(g, hub_team, spoke_count, weight) -> (hub_id, Vec<spoke_ids>)` — one hub endorses N spokes
- `chain(g, team, length, weight) -> Vec<node_ids>` — linear chain of endorsements
- `colluding_ring(g, team, size, weight) -> Vec<node_ids>` — circular endorsement ring
- `healthy_web(g, team, size, density, weight) -> Vec<node_ids>` — interconnected nodes with `density` proportion of possible edges

Each generator names nodes systematically (e.g., `spoke_0`, `chain_2`, `ring_4`) for debugging.

#### `SimulationReport` (`report.rs`)

Runs the engine and collects results.

```rust
pub struct NodeScore {
    pub id: Uuid,
    pub name: String,
    pub team: Team,
    pub distance: Option<f32>,
    pub diversity: i32,
}

pub struct SimulationReport {
    pub anchor_id: Uuid,
    pub scores: Vec<NodeScore>,
}
```

**Methods:**
- `run(g: &GraphBuilder, anchor_id: Uuid) -> Self` — runs `compute_distances_from` + `compute_diversity_from`, merges results with node metadata
- `distance(node_id) -> Option<f32>` — lookup convenience
- `diversity(node_id) -> i32` — lookup convenience
- `red_nodes() -> Vec<&NodeScore>` — filter by team
- `blue_nodes() -> Vec<&NodeScore>` — filter by team
- `format_table() -> String` — ASCII table with team/distance/diversity
- `write_dot(path) -> io::Result<()>` — DOT file with red/blue coloring and score annotations

### Test file

`service/tests/trust_simulation_tests.rs` — named scenarios using `#[shared_runtime_test]` + `isolated_db()`.

## Named Scenarios (initial set)

### 1. Hub-and-spoke Sybil attack
- **Setup:** Blue anchor → Blue bridge → Red hub → 5 Red spokes (all weight 1.0)
- **Assert:** All red spokes have diversity=1. All red spokes have distance ≥ 3.0 (Congress threshold).

### 2. Chain infiltration
- **Setup:** Blue anchor with healthy blue web (5 nodes). Red chain of length 8 attached to one blue node via social referral (0.3).
- **Assert:** Red nodes beyond position ~3 exceed 10.0 distance cutoff (unreachable). Remaining red nodes have diversity=1.

### 3. Colluding ring
- **Setup:** Blue anchor with healthy blue web. Red ring of 6 nodes, attached to blue web at single point.
- **Assert:** Red ring nodes have diversity=1 (internal endorsements don't help because endorsers aren't independently reachable). Ring attachment point is the only path.

### 4. Mixed topology — red cluster at single attachment
- **Setup:** Blue anchor with healthy web (10 nodes, high density). Red cluster (5 nodes, fully connected internally) attached via single blue→red edge.
- **Assert:** Despite high internal density, all red nodes have diversity=1. Blue web nodes have diversity ≥ 2.

### 5. Social referral ceiling
- **Setup:** Blue anchor. Chain of nodes connected only via social referral (weight=0.3, cost=3.33 per hop).
- **Assert:** By hop 3, distance exceeds 10.0 (3.33 × 3 ≈ 10.0). Social-referral-only paths are structurally limited.

### 6. Weight calibration baseline
- **Setup:** Blue anchor with three parallel paths to same target: physical QR (1.0), video (0.7), social (0.3).
- **Assert:** Distances are 1.0, ~1.43, ~3.33 respectively. Target gets diversity=3 (three distinct endorsers).

## Implementation Steps

### Step 1: Extract shared endorsement helpers
Move `insert_endorsement` and `insert_revoked_endorsement` from `trust_engine_tests.rs` into `common/factories/` so both test files can use them. Keep the originals as re-exports to avoid breaking existing tests.

### Step 2: Build GraphBuilder (mod.rs)
The core struct. Depends on step 1 for endorsement insertion.

### Step 3: Build TopologyGenerator (topology.rs)
Parameterized constructors. Depends on step 2.

### Step 4: Build SimulationReport (report.rs)
Engine runner + output formatting. Depends on step 2 (needs `GraphBuilder` for node metadata).

### Step 5: Write named scenario tests
The test file with all 6 scenarios. Depends on steps 2-4.

### Step 6: Run and validate
Execute all scenarios, verify assertions match expectations, review DOT output.

## What this enables next

Once the harness exists, we can:
- **Add denouncement experiments** as parameterized variations on each scenario
- **Compare sponsorship risk mechanisms** by adding penalty logic as graph mutations
- **Regression-test** any trust engine changes against known attack topologies
- **Visualize** complex topologies via DOT output for design discussions

## Existing infrastructure used

| Component | Source | Usage |
|---|---|---|
| `isolated_db()` | `service/tests/common/mod.rs` | Per-test DB isolation |
| `AccountFactory` | `service/tests/common/factories/account.rs` | Deterministic account creation |
| `insert_endorsement` | `service/tests/trust_engine_tests.rs` (to be extracted) | Direct endorsement insertion |
| `TrustEngine` | `service/src/trust/engine.rs` | CTE distance + diversity computation |
| `#[shared_runtime_test]` | `crates/test-macros/src/lib.rs` | Shared tokio runtime for test isolation |
| `ComputedScore` | `service/src/trust/engine.rs` | Score result type |
