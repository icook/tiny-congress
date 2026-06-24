# Design: Pipeline assertions for simulation scenarios

**Date:** 2026-03-13
**Branch:** test/624-trust-simulation-harness (#643)
**Addresses:** Adversarial review finding #3 — simulation validates computation but not the materialization + constraint pipeline.

## Problem

The simulation harness proves the engine computes correct scores (distance, diversity) for adversarial topologies. But the actual access control path is:

```
graph → TrustEngine → trust__score_snapshots → RoomConstraint::check → eligible/ineligible
```

No test runs this full pipeline on an adversarial topology. A bug in `recompute_from_anchor` (serialization, defaults) or in constraint comparison (`>=` vs `>`) would be invisible.

## Existing coverage

- `trust_e2e_tests.rs`: Full pipeline but only trivial 3-node graph + `EndorsedByConstraint` (weakest constraint)
- `trust_constraint_tests.rs`: All three constraints but with hand-crafted snapshots (never computed from a real graph)
- `trust_simulation_tests.rs`: Real adversarial graphs but engine-only (no materialization, no constraints)

## Design

### 1. Extend `SimulationReport` with two methods

**`materialize(&self, anchor_id, pool) -> ()`**
- Calls `recompute_from_anchor(anchor_id, &PgTrustRepo)` to write computed scores to `trust__score_snapshots`
- Must be called explicitly after `run()` — `run()` stays side-effect-free

**`check_eligibility(&self, node_id, anchor_id, constraint, pool) -> Eligibility`**
- Reads from `trust__score_snapshots` via `PgTrustRepo`
- Delegates to `constraint.check(node_id, Some(anchor_id), &repo)`
- Requires `materialize()` to have been called first

### 2. Add pipeline assertions to three scenarios

| Scenario | Constraint | Assertion |
|---|---|---|
| `sim_hub_and_spoke_sybil_attack` | `CommunityConstraint(5.0, 2)` | All 5 spokes **rejected** (diversity=1 < min=2) |
| `sim_colluding_ring` | `CongressConstraint(2)` | All 6 ring nodes **rejected** (diversity=1 < min=2) |
| `sim_weight_calibration` | `CommunityConstraint(5.0, 2)` | Target **eligible** (distance=2.0, diversity=3) |

### 3. What doesn't change

- Existing engine-level assertions remain
- `SimulationReport::run()` behavior unchanged
- Other 3 scenarios (chain, social ceiling, red cluster) don't get pipeline assertions — redundant with the above
- No new test files or scenarios

## Implementation tasks

1. Add `materialize` and `check_eligibility` methods to `SimulationReport` in `service/tests/common/simulation/report.rs`
2. Add pipeline assertions to `sim_hub_and_spoke_sybil_attack`
3. Add pipeline assertions to `sim_colluding_ring`
4. Add pipeline assertions to `sim_weight_calibration`
5. Run `cargo test --test trust_simulation_tests` to verify
6. Fix stale comments referencing "approximation" (review finding #6, low-hanging fruit)
