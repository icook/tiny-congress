# Simulation Pipeline Assertions — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Close the test coverage gap where simulation scenarios validate engine computation but not the materialization + constraint access-control pipeline.

**Architecture:** Extend `SimulationReport` with `materialize()` (writes scores to snapshot table) and `check_eligibility()` (runs a constraint check against snapshots). Add pipeline assertions to 3 existing simulation scenarios.

**Tech Stack:** Rust, sqlx, tinycongress_api trust engine/constraints/repo

---

### Task 1: Add `materialize` and `check_eligibility` to `SimulationReport`

**Files:**
- Modify: `service/tests/common/simulation/report.rs`

**Step 1: Add imports**

Add these imports at the top of `report.rs`:

```rust
use sqlx::PgPool;
use tinycongress_api::trust::constraints::{Eligibility, RoomConstraint};
use tinycongress_api::trust::repo::{PgTrustRepo, TrustRepo};
```

**Step 2: Add `materialize` method to `SimulationReport`**

Add after the `run` method (around line 60):

```rust
    /// Write computed scores to `trust__score_snapshots` via `recompute_from_anchor`.
    ///
    /// Must be called before `check_eligibility`. Separated from `run()` to keep
    /// the default path side-effect-free.
    pub async fn materialize(&self, pool: &PgPool) {
        let engine = TrustEngine::new(pool.clone());
        let repo = PgTrustRepo::new(pool.clone());
        engine
            .recompute_from_anchor(self.anchor_id, &repo)
            .await
            .expect("recompute_from_anchor failed during materialize");
    }
```

**Step 3: Add `check_eligibility` method to `SimulationReport`**

Add after `materialize`:

```rust
    /// Check a node's eligibility against a room constraint.
    ///
    /// Requires `materialize()` to have been called first (reads from snapshot table).
    pub async fn check_eligibility(
        &self,
        node_id: Uuid,
        constraint: &dyn RoomConstraint,
        pool: &PgPool,
    ) -> Eligibility {
        let repo = PgTrustRepo::new(pool.clone());
        constraint
            .check(node_id, Some(self.anchor_id), &repo)
            .await
            .expect("constraint check failed")
    }
```

**Step 4: Verify it compiles**

Run: `cargo test --test trust_simulation_tests --no-run`
Expected: Compiles successfully (no callers yet, methods are just added)

**Step 5: Commit**

```bash
git add service/tests/common/simulation/report.rs
git commit -m "test: add materialize and check_eligibility to SimulationReport"
```

---

### Task 2: Add pipeline assertions to `sim_hub_and_spoke_sybil_attack`

**Files:**
- Modify: `service/tests/trust_simulation_tests.rs`

**Step 1: Add import**

Add to the imports at the top of `trust_simulation_tests.rs`:

```rust
use tinycongress_api::trust::constraints::CommunityConstraint;
```

**Step 2: Add pipeline assertions at the end of `sim_hub_and_spoke_sybil_attack`**

After the existing distance assertion block (before the closing `}`), add:

```rust
    // Pipeline assertion: materialize scores, then verify spokes are
    // rejected by CommunityConstraint (diversity=1 < min_diversity=2).
    report.materialize(db.pool()).await;
    let constraint = CommunityConstraint::new(5.0, 2).expect("valid constraint");
    for &spoke in &spokes {
        let eligibility = report.check_eligibility(spoke, &constraint, db.pool()).await;
        assert!(
            !eligibility.is_eligible,
            "Hub-and-spoke: spoke should be rejected by CommunityConstraint(min_diversity=2)"
        );
    }
```

Note: `report` doesn't exist yet in this function — it currently uses `engine` directly. You need to replace the direct engine calls with `SimulationReport::run` first. Check if the scenario already uses `SimulationReport`. If not, add it:

```rust
    let report = SimulationReport::run(&g, anchor).await;
```

Then use `report.distance(spoke)` and `report.diversity(spoke)` for existing assertions instead of raw engine calls. The hub-and-spoke scenario currently calls engine directly — refactor to use report, then add pipeline assertions.

**Step 3: Run the test**

Run: `cargo test --test trust_simulation_tests sim_hub_and_spoke -- --nocapture`
Expected: PASS — spokes have diversity=1, rejected by CommunityConstraint(min_diversity=2)

**Step 4: Commit**

```bash
git add service/tests/trust_simulation_tests.rs
git commit -m "test: add pipeline assertion to hub-and-spoke scenario"
```

---

### Task 3: Add pipeline assertions to `sim_colluding_ring`

**Files:**
- Modify: `service/tests/trust_simulation_tests.rs`

**Step 1: Add import if not already present**

```rust
use tinycongress_api::trust::constraints::CongressConstraint;
```

**Step 2: Add pipeline assertions at the end of `sim_colluding_ring`**

After the existing distance assertions (before `report.write_dot`), add:

```rust
    // Pipeline assertion: materialize scores, then verify ring nodes are
    // rejected by CongressConstraint (diversity=1 < min_diversity=2).
    report.materialize(db.pool()).await;
    let constraint = CongressConstraint::new(2).expect("valid constraint");
    for red in report.red_nodes() {
        let eligibility = report.check_eligibility(red.id, &constraint, db.pool()).await;
        assert!(
            !eligibility.is_eligible,
            "Colluding ring: node '{}' should be rejected by CongressConstraint(min_diversity=2), got eligible",
            red.name
        );
    }
```

**Step 3: Run the test**

Run: `cargo test --test trust_simulation_tests sim_colluding_ring -- --nocapture`
Expected: PASS — ring nodes all have diversity=1, rejected by CongressConstraint(min_diversity=2)

**Step 4: Commit**

```bash
git add service/tests/trust_simulation_tests.rs
git commit -m "test: add pipeline assertion to colluding ring scenario"
```

---

### Task 4: Add pipeline assertions to `sim_weight_calibration`

**Files:**
- Modify: `service/tests/trust_simulation_tests.rs`

**Step 1: Add pipeline assertions at the end of `sim_weight_calibration`**

After the existing bridge distance assertions (before `report.write_dot`), add:

```rust
    // Pipeline assertion: materialize scores, then verify target passes
    // CommunityConstraint (distance=2.0 <= 5.0, diversity=3 >= 2).
    report.materialize(db.pool()).await;
    let constraint = CommunityConstraint::new(5.0, 2).expect("valid constraint");
    let eligibility = report.check_eligibility(target, &constraint, db.pool()).await;
    assert!(
        eligibility.is_eligible,
        "Weight calibration: target with distance=2.0 and diversity=3 should pass CommunityConstraint"
    );
```

**Step 2: Run the test**

Run: `cargo test --test trust_simulation_tests sim_weight_calibration -- --nocapture`
Expected: PASS — target has distance=2.0 and diversity=3, passes CommunityConstraint(5.0, 2)

**Step 3: Commit**

```bash
git add service/tests/trust_simulation_tests.rs
git commit -m "test: add pipeline assertion to weight calibration scenario"
```

---

### Task 5: Fix stale approximation comments

**Files:**
- Modify: `service/tests/trust_simulation_tests.rs`

**Step 1: Update comments in `sim_colluding_ring`**

Replace the block starting "Key question: does the diversity approximation..." with:

```rust
    // Vertex connectivity correctly identifies all ring nodes as diversity=1:
    // despite internal endorsements, the ring connects to the anchor through
    // a single bridge node (the only vertex-disjoint path).
```

**Step 2: Update comments in `sim_red_cluster_single_attachment`**

Replace the block starting "Document the diversity approximation limitation..." with:

```rust
    // Vertex connectivity correctly identifies all red cluster nodes as
    // diversity=1: despite being fully connected internally, the cluster
    // connects to the anchor through a single blue bridge node.
```

Also update the eprintln to remove "(approximation inflated)":

```rust
        eprintln!(
            "  Red cluster '{}': distance={:?}, diversity={}",
            red.name, red.distance, red.diversity
        );
```

**Step 3: Run all simulation tests**

Run: `cargo test --test trust_simulation_tests -- --nocapture`
Expected: All 6 scenarios PASS (this is a comment-only change + eprintln tweak)

**Step 4: Commit**

```bash
git add service/tests/trust_simulation_tests.rs
git commit -m "docs: update simulation comments to reflect vertex connectivity"
```

---

### Task 6: Final verification

**Step 1: Run all trust tests**

Run: `cargo test --test trust_simulation_tests --test trust_engine_tests --test trust_sybil_tests --test trust_e2e_tests --test trust_constraint_tests -- --nocapture`
Expected: All tests pass

**Step 2: Run lint**

Run: `just lint-backend`
Expected: Clean

**Step 3: Push**

```bash
git push origin test/624-trust-simulation-harness
```
