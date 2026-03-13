# Implementation Plan: Simulation-Driven ADR Acceptance

**Date:** 2026-03-13
**Branch:** sim/open-questions-workspace
**Goal:** Use the trust simulation harness to resolve open questions and advance ADR-024 (Draft→Accepted) and ADR-025 (Draft→Accepted). Stress-test ADR-023's weight table.
**Prerequisite:** PR #673 (GraphSpec extraction) merged — provides `GraphSpec`, behavioral predicates, proptest generators, and temporal extensions.

---

## Phase 1: Validate denouncer-only revocation → accept ADR-024

**Open questions resolved:** Q8 (penalty values), Q16 (propagation depth), Q18 (proportionality), Q19 (circular cascades)
**Deliverables:** New mechanism in `mechanisms.rs`, 4+ new test scenarios, updated comparison table, ADR-024 accepted with evidence

### Step 1: Add `apply_denouncer_revocation` to mechanisms.rs

Add a 4th mechanism function alongside the existing three. Unlike `apply_edge_removal` (which revokes ALL inbound edges), this revokes only the denouncer→target edge.

```rust
pub async fn apply_denouncer_revocation(
    g: &mut GraphBuilder,
    denouncer: Uuid,
    target: Uuid,
    anchor: Uuid,
    pool: &PgPool,
) -> SimulationReport { ... }
```

Implementation: call `g.revoke(denouncer, target)`, then `SimulationReport::run(&g, anchor)` + `materialize(pool)`.

**Files:** `service/tests/common/simulation/mechanisms.rs`

### Step 2: Add denouncer-only to comparison framework

Extend `sim_mechanism_comparison` and `sim_weaponization_resistance` to include `"denouncer_revocation"` as a 4th mechanism in the loop.

For hub-and-spoke: the denouncer is `bridge` (the blue node that endorses `red_hub`).
For mercenary: the denouncer is `blue_web[0]` (one of the three endorsers).
For weaponization: the denouncer is `red_hub` (simulates a Sybil denouncing the blue target — should have minimal effect since Sybil may not even have an edge to target).

**Files:** `service/tests/trust_simulation_tests.rs`

### Step 3: New scenario — coordinated denouncement

Topology: well-connected red target with 4 blue endorsers (diversity=4). Three independent blue nodes each denounce the target (revoke their edge). Target should drop to diversity=1, losing eligibility.

Purpose: proves denouncer-only revocation CAN remove bad actors when enough independent actors agree — the "soft consensus" property.

```
anchor → bridge_a → target (red)
anchor → bridge_b → target
anchor → bridge_c → target
anchor → bridge_d → target
```

Denounce from bridge_a, bridge_b, bridge_c. Assert: target diversity drops from 4→1, loses eligibility. bridge_d's edge remains.

**Files:** `service/tests/trust_simulation_tests.rs`

### Step 4: New scenario — insufficient denouncement (single denouncer)

Same topology as Step 3. Only bridge_a denounces. Assert: target diversity drops from 4→3, RETAINS eligibility. Proves proportionality — a single actor cannot unilaterally remove a well-connected node.

**Files:** `service/tests/trust_simulation_tests.rs`

### Step 5: Simulate propagation alongside denouncer-only

Add a combo mechanism: `apply_denouncer_revocation_with_cascade` — revokes denouncer→target edge, THEN applies sponsorship cascade penalties to other endorsers of the target.

New test: `sim_propagation_comparison` — runs all mechanisms against the mercenary-bot topology, comparing:
- Denouncer-only alone (soft: target loses 1 path)
- Denouncer-only + cascade (harder: target loses 1 path, other endorsers penalized)
- Full cascade alone (existing: edges revoked + penalties on endorsers)

Sub-tests for propagation design questions:
- **Q16 one-hop vs multi-hop:** Test cascade with depth=1 (direct endorsers only) vs depth=2 (endorsers of endorsers). Use chain topology where bad actor is 3 hops from anchor — does multi-hop cascade reach the anchor's direct endorsees?
- **Q18 proportionality:** Sweep cascade penalty values (1.0/1, 2.0/1, 3.0/1 distance/diversity) and record blue casualties at each level. The loss function (Q7, resolved: bias defensive) tells us to accept the level with fewest false negatives.
- **Q19 circular cascades:** Build A→B→C→A ring topology. Apply cascade to A. Verify penalties don't loop (endorser penalty should not re-trigger cascade on the next hop). If the current implementation lacks visited-set protection, add it.

**Files:** `service/tests/common/simulation/mechanisms.rs`, `service/tests/trust_simulation_tests.rs`

### Step 6: Accept ADR-024

Update `docs/decisions/024-denouncement-mechanism.md`:
- Status: Draft → Accepted
- Add "Simulation Evidence" section with comparison table output
- Document the combo mechanism decision (denouncer-only + cascade for propagation)
- Record the cascade penalty values chosen from the sweep
- Note the circular cascade finding (Q19)

**Files:** `docs/decisions/024-denouncement-mechanism.md`

---

## Phase 2: Weight variance → stress-test ADR-023

**Open questions resolved:** Q4 (weight variance), Q5 (calibration criteria), Q6 (gameable self-reporting)
**Deliverables:** Mixed-weight adversarial scenarios, weight sweep results, go/no-go on ADR-023 weight table

### Step 7: Mixed-weight adversarial scenarios

Create variants of the 3 baseline adversarial scenarios (hub-and-spoke, mercenary, colluding ring) where edge weights are drawn from ADR-023's table instead of uniform 1.0:

| Swap method | Relationship | Weight |
|---|---|---|
| QR code | Years | 1.0 |
| Video call | Months | 0.49 |
| Text message | Acquaintance | 0.2 |
| Email link | Acquaintance | 0.1 |

For Sybil scenarios: assume attacker claims maximum weight (1.0) on all edges — worst case for gameable self-reporting (Q6).
For legitimate network: use mixed weights reflecting realistic distributions.

Run denouncer-only + cascade against each. Record whether red nodes are still blocked and blue nodes are still reachable.

**Files:** `service/tests/trust_simulation_tests.rs`

### Step 8: Weight sweep on mercenary-bot

Parameterize the mercenary-bot scenario: vary the weight of the 3 endorsement edges from 0.1 to 1.0 in 0.1 increments. For each weight level, record:
- Mercenary's distance and diversity
- Whether mercenary passes CommunityConstraint(5.0, 2)
- After denouncer-only + cascade: whether mercenary loses eligibility

Acceptance criterion (Q5): "All adversarial scenarios still produce the expected outcome (red blocked, blue passes) across the weight range."

**Files:** `service/tests/trust_simulation_tests.rs`

### Step 9: Confirm max-weight Sybil still fails

Explicit test: Sybil hub-and-spoke with all edges at 1.0. Verify spokes still fail diversity check (diversity=1 regardless of weight). This closes Q6 — the DB weight cap + fixed slot cost + diversity metric together bound the damage from gameable self-reporting.

**Files:** `service/tests/trust_simulation_tests.rs`

---

## Phase 3: Time decay experiments → accept ADR-025

**Open questions resolved:** Q12 (decay model), Q14 (slot auto-release), Q15 (simulation coverage)
**Deliverables:** Decay function recommendation with data, temporal adversarial scenarios, ADR-025 accepted

### Step 10: Define 3 candidate decay functions

Implement as closures matching the `apply_decay` signature `Fn(chrono::Duration) -> f32`:

```rust
// Exponential: half-life of 6 months
let exponential = |age: Duration| -> f32 {
    let months = age.num_days() as f64 / 30.0;
    ((-0.693 / 6.0) * months).exp() as f32  // ln(2)/half_life
};

// Step function: full weight for 1yr, half for 1-2yr, zero after 2yr
let step = |age: Duration| -> f32 {
    let days = age.num_days();
    if days < 365 { 1.0 }
    else if days < 730 { 0.5 }
    else { 0.0 }
};

// Linear: decreases to zero over 2 years
let linear = |age: Duration| -> f32 {
    let fraction = age.num_days() as f32 / 730.0;
    (1.0 - fraction).max(0.0)
};
```

**Files:** `service/tests/trust_simulation_tests.rs` (or a new `service/tests/trust_decay_tests.rs`)

### Step 11: Temporal adversarial scenario — Sybil attack window

Build two networks using `hub_and_spoke_temporal`:
- **Sybil cluster:** All edges created 1 week ago (fresh)
- **Organic network:** Edges created 1-12 months ago (aged)

Apply each decay function at `now`. Measure:
- Does the Sybil cluster retain full weight while organic network decays?
- After 6 months of simulated time: does the Sybil cluster's edges decay enough to lose eligibility?
- What is the "attack window" — how long does a Sybil cluster remain effective before decay neuters it?

This answers Q15 (simulation coverage for temporal dimension).

**Files:** `service/tests/trust_simulation_tests.rs`

### Step 12: Stale-but-legitimate edges

Build a blue network where edges range from 6 months to 2 years old. Apply each decay function. Check:
- Do well-connected nodes (diversity ≥ 3) remain eligible after decay?
- At what age do edges become too weak to contribute? (Identifies renewal pressure)
- Does the step function preserve legitimate networks better than exponential? (Step gives full weight for the first year)

**Files:** `service/tests/trust_simulation_tests.rs`

### Step 13: Slot auto-release policy (Q14)

Test: a node has k=10 slots, all occupied by edges aged 2+ years. Under each decay function:
- How many edges have weight ≈ 0? (These are "dead slots")
- If we auto-release below weight 0.05: how many slots free up?
- Does auto-release change the node's distance/diversity? (It shouldn't if the edge was already negligible)

The answer informs whether to auto-release or require explicit revocation.

**Files:** `service/tests/trust_simulation_tests.rs`

### Step 14: Accept ADR-025

Update `docs/decisions/025-trust-edge-time-decay.md`:
- Status: Draft → Accepted
- Record the chosen decay function with simulation evidence
- Document attack window findings
- Record the renewal pressure threshold (at what age users should re-swap)
- Document slot auto-release decision (Q14)

**Files:** `docs/decisions/025-trust-edge-time-decay.md`

---

## Execution notes

- **Branch strategy:** Each phase gets its own branch off master. Phase 1 is highest priority. Phases 2-3 are independent and can run in parallel after Phase 1.
- **Test output:** All comparison tables write to `target/simulation/` for inspection. DOT files for visual debugging.
- **Loss function applied throughout:** Per Q7 resolution, accept blue casualties when they come with red blocking. `W_block >> W_collateral`.
- **PR #673 must merge first** — Phase 1 needs `GraphSpec` for the new predicate-driven tests and Phase 3 needs temporal extensions.
