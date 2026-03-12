# Trust Graph Red/Blue Sybil Resistance Simulation

> **Status:** Design in progress. Blocked on finalizing trust handshake base abstractions.

**Issue:** #624
**Goal:** Validate the trust engine's Sybil resistance claims by simulating adversarial graph topologies against the real service layer.

---

## Decisions Made

- **Audience:** Developer/design tool (not user-facing, not demo artifact)
- **Primary scenario:** Sybil attack resistance — red team creates fake accounts in various topologies, observe whether they gain trust distance / path diversity / room eligibility
- **Where it runs:** Rust integration tests using the real service layer (`TrustEngine`, `TrustService`, constraints) against `isolated_db()` test databases
- **Approach:** Parameterized topology generators + named regression scenarios (Approach B from brainstorming)
- **Output:** Pass/fail assertions + structured summary table + DOT/Graphviz files with red/blue coloring and score annotations

## Architecture (Approved)

**Module location:** `service/tests/common/simulation/`

**Components:**
- `GraphBuilder` — programmatic topology construction (nodes with red/blue team designation, weighted directed edges, convenience methods like `add_clique`)
- `TopologyGenerator` — parameterized generators: `hub_and_spoke(hub, spoke_count)`, `chain(length)`, `colluding_ring(size)`, `healthy_web(size, density)`
- `SimulationReport` — runs `TrustEngine` from each anchor, collects scores, produces summary table + DOT file to `target/simulation/`

**Named scenarios (initial set TBD):**
- Hub-and-spoke Sybil attack
- Chain infiltration
- Colluding ring
- Mixed: healthy web + red cluster attached at single point
- (More to be defined after handshake abstractions solidify)

**Test pattern:** `#[shared_runtime_test]` + `isolated_db()` + direct `reputation__endorsements` inserts (same as existing `trust_engine_tests.rs`)

## Open Questions

- **GraphBuilder node model:** Initial proposal used `enum Team { Blue, Red }` and `SimNode { id, name, team }`. User flagged this needs rethinking after handshake base abstractions are finalized — the simulation model should align with whatever the handshake establishes.
- **Topology generators:** Exact parameters and default values TBD.
- **Assertion invariants:** Need to define the specific properties we expect to hold (e.g., "red hub-and-spoke nodes never achieve `path_diversity >= 2`", "red chain of length > N exceeds distance cutoff"). These should be derived from the trust engine's actual algorithm properties.
- **Denouncement simulation:** Denouncements don't currently affect graph traversal. Should the simulation test what *would* happen if they did, or only test current behavior?

## Context

### Trust engine properties (from exploration)
- Weighted directed graph: edges in `reputation__endorsements`, cost = `1/weight`
- Recursive CTE with distance cutoff at 10.0, cycle prevention via path array
- Path diversity = count of distinct reachable endorsers (approximation, not true edge-disjoint)
- Influence budgets: 10.0 default, staked on endorsements, burned on denouncements
- Denouncements stored but **do not affect traversal** yet
- Room constraints check distance + diversity thresholds

### Existing test infrastructure
- `isolated_db()` for per-test DB isolation (PostgreSQL template copy)
- `AccountFactory::new().with_seed(N)` for deterministic account creation
- Direct endorsement insert helpers already used in `trust_engine_tests.rs`
- `TrustEngine::new(pool)` callable directly from tests

## AI Tooling

Brainstormed with Claude Code (Opus 4.6). Design paused pending handshake abstraction work.
