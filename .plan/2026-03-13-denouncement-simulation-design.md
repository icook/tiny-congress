# Design: Denouncement mechanism simulation

**Date:** 2026-03-13
**Branch:** test/624-trust-simulation-harness (#643)
**Depends on:** Pipeline assertions (same branch), adversarial review findings #5a-e
**Goal:** Expand the simulation harness with adversarial scenarios, then use it to evaluate three candidate denouncement mechanisms by comparing their effects on the same topologies.

## Problem

Denouncements are fully implemented at the backend layer (d=2 permanent budget, `POST /trust/denounce`, worker processing) but have **no effect on the trust graph**. ADR-020 defers the penalty system design. Before implementing a mechanism in the engine, we need empirical data on how each candidate performs against adversarial topologies — blocking bad actors, resisting weaponization, protecting innocents.

## Approach

Two sequential phases on a single branch. Phase 1 adds topology scenarios. Phase 2a simulates denouncement mechanisms in test code (no engine changes) and produces a scored comparison report.

---

## Phase 1: New topology scenarios

Six new named scenarios in `trust_simulation_tests.rs`:

| Scenario | Topology | Key assertion |
|---|---|---|
| `sim_multi_point_attachment` | Red cluster (5 fully-connected) attached to blue network at 2 bridge nodes | Red nodes get diversity=2, *pass* `CommunityConstraint(5.0, 2)` — multi-point attachment defeats diversity checks |
| `sim_asymmetric_weight_exploit` | Red node endorsed by single blue node at weight=10.0 (cost=0.1) | distance≈0.1 but diversity=1 — passes distance, fails diversity. High weight can't substitute for structural diversity |
| `sim_graph_splitting` | Blue network with cut vertex. Measure, then revoke cut vertex edges, re-measure | Downstream blue nodes become unreachable or lose diversity. Quantifies collateral damage from node removal |
| `sim_phantom_edges` | Red node endorsed at weight=0.001 (cost=1000.0, beyond cutoff) | Red node unreachable despite having a DB edge. Distance cutoff rejects phantom edges |
| `sim_coerced_handshake` | Blue web (5 nodes), one "coercer" with 3 forced endorsements from blue nodes | Coercer well-integrated (high diversity). Baseline before Phase 2 tests denouncement dislodging |
| `sim_mercenary_bot` | Blue web (6 nodes), one red node endorsed by 3 independent blue nodes at weight=1.0 | Red node passes all constraints (distance≈1.0, diversity=3). Baseline proving well-integrated bad actor is indistinguishable by topology alone |

### New infrastructure

**Topology generator:** `fully_connected_cluster(g, prefix, team, size, weight)` — N nodes with all-pairs endorsement edges.

**GraphBuilder method:** `revoke(from, to)` — UPDATEs `revoked_at = now()` on the endorsement, marks `SimEdge` as revoked. Required for `sim_graph_splitting` and all Phase 2 mechanism testing.

All scenarios include pipeline assertions (`materialize` + `check_eligibility`).

---

## Phase 2a: Denouncement mechanism simulation

### Three candidate mechanisms

Each implemented as a test-level helper function — no engine changes.

**1. Edge removal** — `apply_edge_removal(g, target, pool) -> SimulationReport`
- Revokes all inbound edges to the denounced target via `GraphBuilder::revoke()`
- Re-runs full pipeline (engine → materialize → check_eligibility)
- Models: denouncement severs the target's connections

**2. Score penalty** — `apply_score_penalty(g, target, pool, distance_penalty, diversity_penalty) -> SimulationReport`
- Runs engine normally, materializes, then directly UPDATEs the target's snapshot: `trust_distance += penalty`, `path_diversity -= 1` (clamped to 0)
- Re-runs `check_eligibility` against modified snapshot
- Models: denouncement degrades standing without removing edges
- Default penalty values: distance += 3.0, diversity -= 1 per denouncement

**3. Sponsorship cascade** — `apply_sponsorship_cascade(g, target, pool) -> SimulationReport`
- Identifies target's endorsers (nodes that vouched for them)
- Revokes endorser→target edges, applies score penalty to endorsers
- Re-runs full pipeline
- Models: you vouched for a bad actor, you lose standing too

### Scenarios tested

Each mechanism runs against: `sim_hub_and_spoke`, `sim_colluding_ring`, `sim_graph_splitting`, `sim_coerced_handshake`, `sim_mercenary_bot`.

### Weaponization scenario

Dedicated scenario: Sybil cluster (5 red nodes, d=2 budget each = 10 denouncements) targets a single legitimate blue node. Tests whether each mechanism protects blue targets from mass denouncement by adversaries.

---

## Comparison report

### `MechanismComparison` struct

Captures before/after metrics per scenario × mechanism:

- Target node: distance, diversity, eligibility (before and after)
- Collateral damage: blue nodes that lost eligibility (count / total)
- Weaponization resistance: did the blue target survive mass denouncement (where applicable)

### `ComparisonTable`

Collects `MechanismComparison` rows, prints summary table via `Display`:

```
Scenario                  Mechanism              Target lost access?  Blue casualties  Weaponized?
─────────────────────────────────────────────────────────────────────────────────────────────────
hub_and_spoke             edge_removal           yes                  0/3              —
hub_and_spoke             score_penalty          yes                  0/3              —
hub_and_spoke             sponsorship_cascade    yes                  1/3              —
coerced_handshake         edge_removal           yes                  2/5              —
mercenary_bot             edge_removal           yes                  0/6              —
```

Output to stderr (`--nocapture`) and `target/simulation/mechanism_comparison.txt`.

---

## What doesn't change

- No engine changes — all mechanism simulation is test-level graph/snapshot mutation
- No new migrations — denouncement table already exists
- No new test files — everything in `trust_simulation_tests.rs` + `common/simulation/`
- Existing 6 scenarios unchanged
- Phase 2b (implement winning mechanism in engine) is out of scope

## Out of scope

- Choosing and implementing the winning mechanism (Phase 2b — separate design after reviewing comparison data)
- Frontend denouncement UI
- ADR updates for the selected mechanism
- The "partial-budget ordering" question from the ADR audit (ADR-020 vs 021)
