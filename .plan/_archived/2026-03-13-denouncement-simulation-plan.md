# Denouncement Mechanism Simulation — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Expand the simulation harness with 6 adversarial scenarios and a denouncement mechanism comparison framework that evaluates edge removal, score penalty, and sponsorship cascade against the same topologies.

**Architecture:** Phase 1 adds topology scenarios with `revoke()` and `fully_connected_cluster` primitives. Phase 2a simulates three denouncement mechanisms as test-level graph mutations, collects before/after metrics in `MechanismComparison` structs, and prints a scored comparison table.

**Tech Stack:** Rust, sqlx, tinycongress_api trust engine/constraints/repo, `#[shared_runtime_test]` + `isolated_db()`

---

## Phase 1: New scenarios and infrastructure

### Task 1: Add `revoke` method to `GraphBuilder`

**Files:**
- Modify: `service/tests/common/simulation/mod.rs`

**Step 1: Add the `revoke` method**

Add after the `endorse_revoked` method (around line 103):

```rust
    /// Revoke an existing endorsement (set revoked_at = now()).
    ///
    /// Panics if no active endorsement exists from→to.
    pub async fn revoke(&mut self, from: Uuid, to: Uuid) {
        let result = sqlx::query(
            "UPDATE reputation__endorsements SET revoked_at = now() \
             WHERE endorser_id = $1 AND subject_id = $2 AND topic = 'trust' AND revoked_at IS NULL",
        )
        .bind(from)
        .bind(to)
        .execute(&self.pool)
        .await
        .expect("revoke endorsement query failed");
        assert_eq!(
            result.rows_affected(),
            1,
            "expected to revoke exactly 1 endorsement from {} to {}",
            from,
            to
        );
        // Mark in local edge list
        if let Some(edge) = self.edges.iter_mut().find(|e| e.from == from && e.to == to && !e.revoked) {
            edge.revoked = true;
        }
    }
```

**Step 2: Verify it compiles**

Run: `cargo test --test trust_simulation_tests --no-run`
Expected: Compiles (no callers yet)

**Step 3: Commit**

```bash
git add service/tests/common/simulation/mod.rs
git commit -m "test: add revoke method to GraphBuilder"
```

---

### Task 2: Add `fully_connected_cluster` topology generator

**Files:**
- Modify: `service/tests/common/simulation/topology.rs`

**Step 1: Add the generator**

Add after the `healthy_web` function:

```rust
/// Create a fully connected cluster: every node endorses every other node.
///
/// Returns all node IDs.
pub async fn fully_connected_cluster(
    g: &mut GraphBuilder,
    prefix: &str,
    team: Team,
    size: usize,
    weight: f32,
) -> Vec<Uuid> {
    let mut nodes = Vec::with_capacity(size);
    for i in 0..size {
        let node = g.add_node(&format!("{prefix}_cluster_{i}"), team).await;
        nodes.push(node);
    }
    for i in 0..size {
        for j in 0..size {
            if i != j {
                g.endorse(nodes[i], nodes[j], weight).await;
            }
        }
    }
    nodes
}
```

**Step 2: Verify it compiles**

Run: `cargo test --test trust_simulation_tests --no-run`
Expected: Compiles

**Step 3: Commit**

```bash
git add service/tests/common/simulation/topology.rs
git commit -m "test: add fully_connected_cluster topology generator"
```

---

### Task 3: Add `sim_multi_point_attachment` scenario

**Files:**
- Modify: `service/tests/trust_simulation_tests.rs`

**Step 1: Add the scenario**

Add after `sim_weight_calibration`:

```rust
// ---------------------------------------------------------------------------
// Scenario 7: Multi-point attachment
//
// Topology:
//   anchor (blue) → bridge_a (blue), anchor → bridge_b (blue)
//   bridge_a → red_cluster[0], bridge_b → red_cluster[1]
//   Red cluster: 5 fully connected nodes
//
// Red cluster attached at TWO blue nodes — diversity=2 for red nodes.
// This demonstrates that multi-point attachment defeats diversity checks
// when min_diversity=2.
// ---------------------------------------------------------------------------
#[shared_runtime_test]
async fn sim_multi_point_attachment() {
    let db = isolated_db().await;
    let mut g = GraphBuilder::new(db.pool().clone());

    // Blue team
    let anchor = g.add_node("anchor", Team::Blue).await;
    let bridge_a = g.add_node("bridge_a", Team::Blue).await;
    let bridge_b = g.add_node("bridge_b", Team::Blue).await;
    g.endorse(anchor, bridge_a, 1.0).await;
    g.endorse(anchor, bridge_b, 1.0).await;

    // Red team: fully connected cluster attached at two points
    let red_cluster =
        topology::fully_connected_cluster(&mut g, "red", Team::Red, 5, 1.0).await;
    g.endorse(bridge_a, red_cluster[0], 0.3).await;
    g.endorse(bridge_b, red_cluster[1], 0.3).await;

    let report = SimulationReport::run(&g, anchor).await;
    eprintln!("\n=== Multi-Point Attachment ===\n{report}");

    // Assert: red nodes are reachable
    for red in report.red_nodes() {
        assert!(
            red.distance.is_some(),
            "Red cluster node '{}' should be reachable",
            red.name
        );
    }

    // Assert: red nodes get diversity=2 (two independent bridge paths)
    for red in report.red_nodes() {
        assert_eq!(
            red.diversity, 2,
            "Multi-point: red node '{}' should have diversity=2, got {}",
            red.name, red.diversity
        );
    }

    // Pipeline assertion: red nodes PASS CommunityConstraint(5.0, 2)
    // This is the dangerous case — adversaries that meet the threshold.
    report.materialize(db.pool()).await;
    let constraint = CommunityConstraint::new(5.0, 2).expect("valid constraint");
    for red in report.red_nodes() {
        let eligibility = report
            .check_eligibility(red.id, &constraint, db.pool())
            .await;
        assert!(
            eligibility.is_eligible,
            "Multi-point: red node '{}' should PASS CommunityConstraint(5.0, 2) — this is the attack succeeding",
            red.name
        );
    }

    // But CongressConstraint with higher diversity should still block
    let strict_constraint = CongressConstraint::new(3).expect("valid constraint");
    for red in report.red_nodes() {
        let eligibility = report
            .check_eligibility(red.id, &strict_constraint, db.pool())
            .await;
        assert!(
            !eligibility.is_eligible,
            "Multi-point: red node '{}' should be rejected by CongressConstraint(min_diversity=3)",
            red.name
        );
    }

    report
        .write_dot(
            &g,
            std::path::Path::new("target/simulation/multi_point_attachment.dot"),
        )
        .expect("write DOT");
}
```

**Step 2: Run the test**

Run: `cargo test --test trust_simulation_tests sim_multi_point_attachment -- --nocapture`
Expected: PASS

**Step 3: Commit**

```bash
git add service/tests/trust_simulation_tests.rs
git commit -m "test: add multi-point attachment scenario"
```

---

### Task 4: Add `sim_asymmetric_weight_exploit` scenario

**Files:**
- Modify: `service/tests/trust_simulation_tests.rs`

**Step 1: Add the scenario**

```rust
// ---------------------------------------------------------------------------
// Scenario 8: Asymmetric weight exploitation
//
// Topology:
//   anchor (blue) → compromised_bridge (blue) at weight 1.0
//   compromised_bridge → red_node at weight 10.0 (super high)
//
// The red node is very "close" (distance ≈ 1.1) but has diversity=1.
// Tests that high edge weight cannot substitute for structural diversity.
// ---------------------------------------------------------------------------
#[shared_runtime_test]
async fn sim_asymmetric_weight_exploit() {
    let db = isolated_db().await;
    let mut g = GraphBuilder::new(db.pool().clone());

    let anchor = g.add_node("anchor", Team::Blue).await;
    let bridge = g.add_node("compromised_bridge", Team::Blue).await;
    g.endorse(anchor, bridge, 1.0).await;

    // Red node with extremely high weight endorsement
    let red = g.add_node("red_exploiter", Team::Red).await;
    g.endorse(bridge, red, 10.0).await; // cost = 1/10.0 = 0.1

    let report = SimulationReport::run(&g, anchor).await;
    eprintln!("\n=== Asymmetric Weight Exploit ===\n{report}");

    // Assert: red node is very close (distance ≈ 1.0 + 0.1 = 1.1)
    let red_dist = report.distance(red).expect("red should be reachable");
    assert!(
        red_dist < 1.5,
        "Asymmetric weight: red node should be very close (d < 1.5), got {red_dist:.3}"
    );

    // Assert: diversity = 1 (only one path)
    let red_div = report.diversity(red);
    assert_eq!(
        red_div, 1,
        "Asymmetric weight: red node should have diversity=1, got {red_div}"
    );

    // Pipeline: passes EndorsedByConstraint (just needs to be reachable)
    // but fails CommunityConstraint (diversity=1 < min=2)
    report.materialize(db.pool()).await;
    let community = CommunityConstraint::new(5.0, 2).expect("valid constraint");
    let eligibility = report
        .check_eligibility(red, &community, db.pool())
        .await;
    assert!(
        !eligibility.is_eligible,
        "Asymmetric weight: red node should fail CommunityConstraint despite close distance"
    );

    report
        .write_dot(
            &g,
            std::path::Path::new("target/simulation/asymmetric_weight_exploit.dot"),
        )
        .expect("write DOT");
}
```

**Step 2: Run the test**

Run: `cargo test --test trust_simulation_tests sim_asymmetric_weight_exploit -- --nocapture`
Expected: PASS

**Step 3: Commit**

```bash
git add service/tests/trust_simulation_tests.rs
git commit -m "test: add asymmetric weight exploit scenario"
```

---

### Task 5: Add `sim_phantom_edges` scenario

**Files:**
- Modify: `service/tests/trust_simulation_tests.rs`

**Step 1: Add the scenario**

```rust
// ---------------------------------------------------------------------------
// Scenario 9: Phantom edges (near-zero weight)
//
// Topology:
//   anchor (blue) → bridge (blue) at weight 1.0
//   bridge → red_node at weight 0.001 (cost = 1000.0, beyond 10.0 cutoff)
//
// An endorsement with near-zero weight creates a DB edge that is
// functionally nonexistent — the distance cutoff should exclude it.
// ---------------------------------------------------------------------------
#[shared_runtime_test]
async fn sim_phantom_edges() {
    let db = isolated_db().await;
    let mut g = GraphBuilder::new(db.pool().clone());

    let anchor = g.add_node("anchor", Team::Blue).await;
    let bridge = g.add_node("bridge", Team::Blue).await;
    g.endorse(anchor, bridge, 1.0).await;

    let red = g.add_node("red_phantom", Team::Red).await;
    g.endorse(bridge, red, 0.001).await; // cost = 1000.0

    let report = SimulationReport::run(&g, anchor).await;
    eprintln!("\n=== Phantom Edges ===\n{report}");

    // Assert: red node is unreachable (distance 1.0 + 1000.0 > 10.0 cutoff)
    assert!(
        report.distance(red).is_none(),
        "Phantom edge: red node should be unreachable (cost=1000.0 exceeds cutoff)"
    );

    // Assert: diversity = 0 (not in reachable set)
    assert_eq!(
        report.diversity(red),
        0,
        "Phantom edge: red node should have diversity=0"
    );

    // Pipeline: materialize and verify red is ineligible
    report.materialize(db.pool()).await;
    let constraint = CommunityConstraint::new(5.0, 2).expect("valid constraint");
    let eligibility = report
        .check_eligibility(red, &constraint, db.pool())
        .await;
    assert!(
        !eligibility.is_eligible,
        "Phantom edge: unreachable red node should fail all constraints"
    );

    report
        .write_dot(
            &g,
            std::path::Path::new("target/simulation/phantom_edges.dot"),
        )
        .expect("write DOT");
}
```

**Step 2: Run the test**

Run: `cargo test --test trust_simulation_tests sim_phantom_edges -- --nocapture`
Expected: PASS

**Step 3: Commit**

```bash
git add service/tests/trust_simulation_tests.rs
git commit -m "test: add phantom edges scenario"
```

---

### Task 6: Add `sim_graph_splitting` scenario

**Files:**
- Modify: `service/tests/trust_simulation_tests.rs`

**Step 1: Add the scenario**

This scenario uses `revoke()` to test before/after effects of removing a cut vertex:

```rust
// ---------------------------------------------------------------------------
// Scenario 10: Graph-splitting attack
//
// Topology:
//   anchor (blue) → cut_vertex (blue) → downstream_a, downstream_b (blue)
//   anchor → alt_bridge (blue) → downstream_a (provides second path to a)
//
// cut_vertex is the only path to downstream_b but NOT to downstream_a.
// Revoking cut_vertex's edges should disconnect downstream_b but not
// downstream_a (which has an alternative path via alt_bridge).
// ---------------------------------------------------------------------------
#[shared_runtime_test]
async fn sim_graph_splitting() {
    let db = isolated_db().await;
    let mut g = GraphBuilder::new(db.pool().clone());

    let anchor = g.add_node("anchor", Team::Blue).await;
    let cut_vertex = g.add_node("cut_vertex", Team::Blue).await;
    let alt_bridge = g.add_node("alt_bridge", Team::Blue).await;
    let downstream_a = g.add_node("downstream_a", Team::Blue).await;
    let downstream_b = g.add_node("downstream_b", Team::Blue).await;

    g.endorse(anchor, cut_vertex, 1.0).await;
    g.endorse(anchor, alt_bridge, 1.0).await;
    g.endorse(cut_vertex, downstream_a, 1.0).await;
    g.endorse(cut_vertex, downstream_b, 1.0).await;
    g.endorse(alt_bridge, downstream_a, 1.0).await;
    // No alt path to downstream_b

    // --- Before revocation ---
    let report_before = SimulationReport::run(&g, anchor).await;
    eprintln!("\n=== Graph Splitting (before) ===\n{report_before}");

    // Both downstream nodes reachable
    assert!(
        report_before.distance(downstream_a).is_some(),
        "Before: downstream_a should be reachable"
    );
    assert!(
        report_before.distance(downstream_b).is_some(),
        "Before: downstream_b should be reachable"
    );
    // downstream_a has diversity=2 (cut_vertex + alt_bridge)
    assert_eq!(
        report_before.diversity(downstream_a),
        2,
        "Before: downstream_a should have diversity=2"
    );
    // downstream_b has diversity=1 (only cut_vertex)
    assert_eq!(
        report_before.diversity(downstream_b),
        1,
        "Before: downstream_b should have diversity=1"
    );

    // Pipeline: materialize and check before state
    report_before.materialize(db.pool()).await;
    let constraint = CommunityConstraint::new(5.0, 2).expect("valid constraint");
    let a_before = report_before
        .check_eligibility(downstream_a, &constraint, db.pool())
        .await;
    assert!(a_before.is_eligible, "Before: downstream_a should be eligible (div=2)");
    let b_before = report_before
        .check_eligibility(downstream_b, &constraint, db.pool())
        .await;
    assert!(!b_before.is_eligible, "Before: downstream_b should be ineligible (div=1)");

    // --- Revoke cut_vertex's inbound edge ---
    g.revoke(anchor, cut_vertex).await;

    let report_after = SimulationReport::run(&g, anchor).await;
    eprintln!("\n=== Graph Splitting (after) ===\n{report_after}");

    // downstream_a still reachable via alt_bridge
    assert!(
        report_after.distance(downstream_a).is_some(),
        "After: downstream_a should still be reachable via alt_bridge"
    );
    // downstream_b disconnected (only path was through cut_vertex)
    assert!(
        report_after.distance(downstream_b).is_none(),
        "After: downstream_b should be unreachable (cut_vertex revoked)"
    );
    // downstream_a drops to diversity=1 (only alt_bridge path remains)
    assert_eq!(
        report_after.diversity(downstream_a),
        1,
        "After: downstream_a should drop to diversity=1"
    );

    report_after
        .write_dot(
            &g,
            std::path::Path::new("target/simulation/graph_splitting.dot"),
        )
        .expect("write DOT");
}
```

**Step 2: Run the test**

Run: `cargo test --test trust_simulation_tests sim_graph_splitting -- --nocapture`
Expected: PASS

**Step 3: Commit**

```bash
git add service/tests/trust_simulation_tests.rs
git commit -m "test: add graph-splitting scenario"
```

---

### Task 7: Add `sim_coerced_handshake` scenario

**Files:**
- Modify: `service/tests/trust_simulation_tests.rs`

**Step 1: Add the scenario**

```rust
// ---------------------------------------------------------------------------
// Scenario 11: Coerced handshake (baseline)
//
// Topology:
//   anchor (blue) → 4 blue web nodes (interconnected, density 0.5)
//   3 blue web nodes → coercer (red) at weight 1.0 (forced QR handshakes)
//
// The coercer has real handshakes from real humans — high diversity.
// This is a baseline measurement: the coercer SHOULD pass constraints
// because topologically they look legitimate. Phase 2 tests whether
// denouncement can dislodge them.
// ---------------------------------------------------------------------------
#[shared_runtime_test]
async fn sim_coerced_handshake() {
    let db = isolated_db().await;
    let mut g = GraphBuilder::new(db.pool().clone());

    let anchor = g.add_node("anchor", Team::Blue).await;
    let blue_web = topology::healthy_web(&mut g, "blue", Team::Blue, 4, 0.5, 1.0).await;
    // Anchor endorses first two web nodes
    g.endorse(anchor, blue_web[0], 1.0).await;
    g.endorse(anchor, blue_web[1], 1.0).await;

    // Coercer: forced handshakes from 3 blue nodes
    let coercer = g.add_node("coercer", Team::Red).await;
    g.endorse(blue_web[0], coercer, 1.0).await;
    g.endorse(blue_web[1], coercer, 1.0).await;
    g.endorse(blue_web[2], coercer, 1.0).await;

    let report = SimulationReport::run(&g, anchor).await;
    eprintln!("\n=== Coerced Handshake (baseline) ===\n{report}");

    // Coercer should be close and well-diversified
    let coercer_dist = report.distance(coercer).expect("coercer should be reachable");
    assert!(
        coercer_dist < 4.0,
        "Coerced handshake: coercer should be close (d < 4.0), got {coercer_dist:.3}"
    );
    let coercer_div = report.diversity(coercer);
    assert!(
        coercer_div >= 2,
        "Coerced handshake: coercer should have diversity >= 2, got {coercer_div}"
    );

    // Pipeline: coercer passes CommunityConstraint — this is the problem
    report.materialize(db.pool()).await;
    let constraint = CommunityConstraint::new(5.0, 2).expect("valid constraint");
    let eligibility = report
        .check_eligibility(coercer, &constraint, db.pool())
        .await;
    assert!(
        eligibility.is_eligible,
        "Coerced handshake: coercer should PASS CommunityConstraint (topologically legitimate)"
    );

    report
        .write_dot(
            &g,
            std::path::Path::new("target/simulation/coerced_handshake.dot"),
        )
        .expect("write DOT");
}
```

**Step 2: Run the test**

Run: `cargo test --test trust_simulation_tests sim_coerced_handshake -- --nocapture`
Expected: PASS

**Step 3: Commit**

```bash
git add service/tests/trust_simulation_tests.rs
git commit -m "test: add coerced handshake baseline scenario"
```

---

### Task 8: Add `sim_mercenary_bot` scenario

**Files:**
- Modify: `service/tests/trust_simulation_tests.rs`

**Step 1: Add the scenario**

```rust
// ---------------------------------------------------------------------------
// Scenario 12: Mercenary bot (baseline)
//
// Topology:
//   anchor (blue) → 6 blue web nodes (interconnected)
//   3 independent blue nodes → mercenary (red) at weight 1.0
//
// The mercenary accumulated endorsements through months of legitimate
// participation. It's indistinguishable from a real user by topology.
// Baseline measurement before Phase 2 tests denouncement effectiveness.
// ---------------------------------------------------------------------------
#[shared_runtime_test]
async fn sim_mercenary_bot() {
    let db = isolated_db().await;
    let mut g = GraphBuilder::new(db.pool().clone());

    let anchor = g.add_node("anchor", Team::Blue).await;

    // Blue web: 6 nodes, anchor endorses first 3 directly
    let blue_web = topology::healthy_web(&mut g, "blue", Team::Blue, 6, 0.4, 1.0).await;
    g.endorse(anchor, blue_web[0], 1.0).await;
    g.endorse(anchor, blue_web[1], 1.0).await;
    g.endorse(anchor, blue_web[2], 1.0).await;

    // Mercenary: endorsed by 3 independent blue nodes
    let mercenary = g.add_node("mercenary", Team::Red).await;
    g.endorse(blue_web[0], mercenary, 1.0).await;
    g.endorse(blue_web[3], mercenary, 1.0).await;
    g.endorse(blue_web[5], mercenary, 1.0).await;

    let report = SimulationReport::run(&g, anchor).await;
    eprintln!("\n=== Mercenary Bot (baseline) ===\n{report}");

    // Mercenary should look like a legitimate, well-connected user
    let merc_dist = report
        .distance(mercenary)
        .expect("mercenary should be reachable");
    assert!(
        merc_dist < 4.0,
        "Mercenary bot: should be close (d < 4.0), got {merc_dist:.3}"
    );
    let merc_div = report.diversity(mercenary);
    assert!(
        merc_div >= 2,
        "Mercenary bot: should have diversity >= 2, got {merc_div}"
    );

    // Pipeline: mercenary passes all constraints — indistinguishable
    report.materialize(db.pool()).await;
    let constraint = CommunityConstraint::new(5.0, 2).expect("valid constraint");
    let eligibility = report
        .check_eligibility(mercenary, &constraint, db.pool())
        .await;
    assert!(
        eligibility.is_eligible,
        "Mercenary bot: should PASS CommunityConstraint (topologically legitimate)"
    );

    report
        .write_dot(
            &g,
            std::path::Path::new("target/simulation/mercenary_bot.dot"),
        )
        .expect("write DOT");
}
```

**Step 2: Run the test**

Run: `cargo test --test trust_simulation_tests sim_mercenary_bot -- --nocapture`
Expected: PASS

**Step 3: Commit**

```bash
git add service/tests/trust_simulation_tests.rs
git commit -m "test: add mercenary bot baseline scenario"
```

---

### Task 9: Run all Phase 1 tests

**Step 1: Run all simulation tests**

Run: `cargo test --test trust_simulation_tests -- --nocapture`
Expected: All 12 scenarios PASS

**Step 2: Run lint**

Run: `just lint-backend`
Expected: Clean

---

## Phase 2a: Denouncement mechanism comparison

### Task 10: Add `MechanismComparison` and `ComparisonTable`

**Files:**
- Create: `service/tests/common/simulation/comparison.rs`
- Modify: `service/tests/common/simulation/mod.rs` (add `pub mod comparison;`)

**Step 1: Create the comparison module**

Create `service/tests/common/simulation/comparison.rs`:

```rust
//! Denouncement mechanism comparison framework.
//!
//! Captures before/after metrics for each scenario × mechanism combination
//! and prints a scored comparison table.

use std::fmt;
use std::fs;
use std::path::Path;

/// Before/after metrics for a single scenario × mechanism test.
#[derive(Debug, Clone)]
pub struct MechanismComparison {
    pub scenario: String,
    pub mechanism: String,
    pub target_name: String,
    /// Before denouncement
    pub before_distance: Option<f32>,
    pub before_diversity: i32,
    pub before_eligible: bool,
    /// After denouncement
    pub after_distance: Option<f32>,
    pub after_diversity: i32,
    pub after_eligible: bool,
    /// Collateral: blue nodes that lost eligibility
    pub blue_casualties: usize,
    pub blue_total: usize,
    /// Weaponization: did blue target survive Sybil mass-denouncement?
    /// None if not a weaponization test.
    pub survived_weaponization: Option<bool>,
}

impl MechanismComparison {
    /// Did the mechanism successfully remove the target's access?
    pub fn target_lost_access(&self) -> bool {
        self.before_eligible && !self.after_eligible
    }
}

/// Collects comparison rows and prints a summary table.
pub struct ComparisonTable {
    pub rows: Vec<MechanismComparison>,
}

impl ComparisonTable {
    pub fn new() -> Self {
        Self { rows: Vec::new() }
    }

    pub fn add(&mut self, row: MechanismComparison) {
        self.rows.push(row);
    }

    /// Write the comparison table to a file.
    pub fn write_to(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, format!("{self}"))
    }
}

impl fmt::Display for ComparisonTable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "{:<28} {:<24} {:<8} {:<8} {:<16} {:<16} {:<12}",
            "Scenario", "Mechanism", "d_before", "d_after", "div_before→after", "Target lost?", "Blue casualties"
        )?;
        writeln!(f, "{}", "─".repeat(112))?;
        for row in &self.rows {
            let d_before = row
                .before_distance
                .map_or("—".to_string(), |d| format!("{d:.2}"));
            let d_after = row
                .after_distance
                .map_or("—".to_string(), |d| format!("{d:.2}"));
            let div_change = format!("{}→{}", row.before_diversity, row.after_diversity);
            let lost = if row.target_lost_access() {
                "YES"
            } else {
                "no"
            };
            let casualties = format!("{}/{}", row.blue_casualties, row.blue_total);
            writeln!(
                f,
                "{:<28} {:<24} {:<8} {:<8} {:<16} {:<16} {:<12}",
                row.scenario, row.mechanism, d_before, d_after, div_change, lost, casualties
            )?;
        }
        // Weaponization summary
        let weapon_rows: Vec<_> = self
            .rows
            .iter()
            .filter(|r| r.survived_weaponization.is_some())
            .collect();
        if !weapon_rows.is_empty() {
            writeln!(f, "\n{:<28} {:<24} {:<16}", "Scenario", "Mechanism", "Blue survived?")?;
            writeln!(f, "{}", "─".repeat(68))?;
            for row in weapon_rows {
                let survived = if row.survived_weaponization.unwrap_or(false) {
                    "YES"
                } else {
                    "no"
                };
                writeln!(
                    f,
                    "{:<28} {:<24} {:<16}",
                    row.scenario, row.mechanism, survived
                )?;
            }
        }
        Ok(())
    }
}
```

**Step 2: Register the module**

In `service/tests/common/simulation/mod.rs`, add after `pub mod topology;`:

```rust
pub mod comparison;
```

**Step 3: Verify it compiles**

Run: `cargo test --test trust_simulation_tests --no-run`
Expected: Compiles

**Step 4: Commit**

```bash
git add service/tests/common/simulation/comparison.rs service/tests/common/simulation/mod.rs
git commit -m "test: add MechanismComparison and ComparisonTable"
```

---

### Task 11: Add mechanism helper functions

**Files:**
- Create: `service/tests/common/simulation/mechanisms.rs`
- Modify: `service/tests/common/simulation/mod.rs` (add `pub mod mechanisms;`)

**Step 1: Create the mechanisms module**

Create `service/tests/common/simulation/mechanisms.rs`:

```rust
//! Denouncement mechanism simulators.
//!
//! Each function applies a candidate denouncement mechanism to a graph,
//! re-runs the engine, and returns the updated report. These are test-level
//! simulations — no engine changes.

use sqlx::PgPool;
use uuid::Uuid;

use super::report::SimulationReport;
use super::GraphBuilder;

/// Mechanism 1: Edge removal.
///
/// Revokes all inbound edges to the target, re-runs engine + materialize.
pub async fn apply_edge_removal(
    g: &mut GraphBuilder,
    target: Uuid,
    anchor: Uuid,
    pool: &PgPool,
) -> SimulationReport {
    // Find and revoke all active inbound edges to target
    let inbound: Vec<(Uuid, Uuid)> = g
        .all_edges()
        .iter()
        .filter(|e| e.to == target && !e.revoked)
        .map(|e| (e.from, e.to))
        .collect();
    for (from, to) in inbound {
        g.revoke(from, to).await;
    }
    let report = SimulationReport::run(g, anchor).await;
    report.materialize(pool).await;
    report
}

/// Mechanism 2: Score penalty.
///
/// Runs engine + materialize normally, then directly modifies the target's
/// snapshot row (distance += penalty, diversity -= 1 clamped to 0).
pub async fn apply_score_penalty(
    g: &GraphBuilder,
    target: Uuid,
    anchor: Uuid,
    pool: &PgPool,
    distance_penalty: f32,
    diversity_penalty: i32,
) -> SimulationReport {
    let report = SimulationReport::run(g, anchor).await;
    report.materialize(pool).await;
    // Directly mutate the snapshot
    sqlx::query(
        "UPDATE trust__score_snapshots \
         SET trust_distance = COALESCE(trust_distance, 0) + $1, \
             path_diversity = GREATEST(COALESCE(path_diversity, 0) - $2, 0) \
         WHERE user_id = $3 AND context_user_id = $4",
    )
    .bind(distance_penalty)
    .bind(diversity_penalty)
    .bind(target)
    .bind(anchor)
    .execute(pool)
    .await
    .expect("score penalty UPDATE failed");
    report
}

/// Mechanism 3: Sponsorship cascade.
///
/// Revokes endorser→target edges AND applies score penalty to endorsers.
/// Re-runs engine + materialize.
pub async fn apply_sponsorship_cascade(
    g: &mut GraphBuilder,
    target: Uuid,
    anchor: Uuid,
    pool: &PgPool,
) -> SimulationReport {
    // Find endorsers of the target (active inbound edges)
    let endorsers: Vec<Uuid> = g
        .all_edges()
        .iter()
        .filter(|e| e.to == target && !e.revoked)
        .map(|e| e.from)
        .collect();
    // Revoke endorser→target edges
    for &endorser in &endorsers {
        g.revoke(endorser, target).await;
    }
    // Re-run engine with edges revoked
    let report = SimulationReport::run(g, anchor).await;
    report.materialize(pool).await;
    // Apply penalty to endorsers' snapshots
    for &endorser in &endorsers {
        sqlx::query(
            "UPDATE trust__score_snapshots \
             SET trust_distance = COALESCE(trust_distance, 0) + 2.0, \
                 path_diversity = GREATEST(COALESCE(path_diversity, 0) - 1, 0) \
             WHERE user_id = $1 AND context_user_id = $2",
        )
        .bind(endorser)
        .bind(anchor)
        .execute(pool)
        .await
        .expect("sponsorship penalty UPDATE failed");
    }
    report
}
```

**Step 2: Register the module**

In `service/tests/common/simulation/mod.rs`, add:

```rust
pub mod mechanisms;
```

**Step 3: Verify it compiles**

Run: `cargo test --test trust_simulation_tests --no-run`
Expected: Compiles

**Step 4: Commit**

```bash
git add service/tests/common/simulation/mechanisms.rs service/tests/common/simulation/mod.rs
git commit -m "test: add denouncement mechanism helper functions"
```

---

### Task 12: Add mechanism comparison test

**Files:**
- Modify: `service/tests/trust_simulation_tests.rs`

**Step 1: Add imports**

Add to the imports at the top of `trust_simulation_tests.rs`:

```rust
use common::simulation::comparison::{ComparisonTable, MechanismComparison};
use common::simulation::mechanisms;
```

**Step 2: Add the comparison test**

This test runs each mechanism against the hub-and-spoke, colluding ring, and mercenary bot topologies. Each mechanism needs a fresh graph (because edge removal and sponsorship cascade mutate it), so each runs in its own `isolated_db`.

```rust
// ---------------------------------------------------------------------------
// Mechanism comparison: runs 3 mechanisms against key scenarios
// ---------------------------------------------------------------------------
#[shared_runtime_test]
async fn sim_mechanism_comparison() {
    let mut table = ComparisonTable::new();

    // --- Hub-and-spoke: can mechanisms remove spokes? ---
    for mechanism_name in &["edge_removal", "score_penalty", "sponsorship_cascade"] {
        let db = isolated_db().await;
        let mut g = GraphBuilder::new(db.pool().clone());

        let anchor = g.add_node("anchor", Team::Blue).await;
        let bridge = g.add_node("bridge", Team::Blue).await;
        let extra_blue = g.add_node("extra_blue", Team::Blue).await;
        g.endorse(anchor, bridge, 1.0).await;
        g.endorse(anchor, extra_blue, 1.0).await;

        let hub = g.add_node("red_hub", Team::Red).await;
        g.endorse(bridge, hub, 0.3).await;
        let spoke = g.add_node("red_spoke_0", Team::Red).await;
        g.endorse(hub, spoke, 1.0).await;

        // Before
        let before = SimulationReport::run(&g, anchor).await;
        before.materialize(db.pool()).await;
        let constraint = CommunityConstraint::new(5.0, 2).expect("valid");
        let before_elig = before
            .check_eligibility(hub, &constraint, db.pool())
            .await;

        // Apply mechanism to hub
        let after = match *mechanism_name {
            "edge_removal" => {
                mechanisms::apply_edge_removal(&mut g, hub, anchor, db.pool()).await
            }
            "score_penalty" => {
                mechanisms::apply_score_penalty(&g, hub, anchor, db.pool(), 3.0, 1).await
            }
            "sponsorship_cascade" => {
                mechanisms::apply_sponsorship_cascade(&mut g, hub, anchor, db.pool()).await
            }
            _ => unreachable!(),
        };
        let after_elig = after
            .check_eligibility(hub, &constraint, db.pool())
            .await;

        // Count blue casualties
        let blue_ids: Vec<_> = g.nodes_by_team(Team::Blue);
        let blue_casualties = blue_ids
            .iter()
            .filter(|&&id| {
                let before_ok = before.check_eligibility(id, &constraint, db.pool());
                let after_ok = after.check_eligibility(id, &constraint, db.pool());
                // We need sync access — use the snapshot data directly
                before.diversity(id) >= 2 && after.diversity(id) < 2
            })
            .count();

        table.add(MechanismComparison {
            scenario: "hub_and_spoke".to_string(),
            mechanism: mechanism_name.to_string(),
            target_name: "red_hub".to_string(),
            before_distance: before.distance(hub),
            before_diversity: before.diversity(hub),
            before_eligible: before_elig.is_eligible,
            after_distance: after.distance(hub),
            after_diversity: after.diversity(hub),
            after_eligible: after_elig.is_eligible,
            blue_casualties,
            blue_total: blue_ids.len(),
            survived_weaponization: None,
        });
    }

    // --- Mercenary bot: can mechanisms remove a well-integrated attacker? ---
    for mechanism_name in &["edge_removal", "score_penalty", "sponsorship_cascade"] {
        let db = isolated_db().await;
        let mut g = GraphBuilder::new(db.pool().clone());

        let anchor = g.add_node("anchor", Team::Blue).await;
        let blue_web = topology::healthy_web(&mut g, "blue", Team::Blue, 6, 0.4, 1.0).await;
        g.endorse(anchor, blue_web[0], 1.0).await;
        g.endorse(anchor, blue_web[1], 1.0).await;
        g.endorse(anchor, blue_web[2], 1.0).await;

        let mercenary = g.add_node("mercenary", Team::Red).await;
        g.endorse(blue_web[0], mercenary, 1.0).await;
        g.endorse(blue_web[3], mercenary, 1.0).await;
        g.endorse(blue_web[5], mercenary, 1.0).await;

        let before = SimulationReport::run(&g, anchor).await;
        before.materialize(db.pool()).await;
        let constraint = CommunityConstraint::new(5.0, 2).expect("valid");
        let before_elig = before
            .check_eligibility(mercenary, &constraint, db.pool())
            .await;

        let after = match *mechanism_name {
            "edge_removal" => {
                mechanisms::apply_edge_removal(&mut g, mercenary, anchor, db.pool()).await
            }
            "score_penalty" => {
                mechanisms::apply_score_penalty(&g, mercenary, anchor, db.pool(), 3.0, 1)
                    .await
            }
            "sponsorship_cascade" => {
                mechanisms::apply_sponsorship_cascade(&mut g, mercenary, anchor, db.pool())
                    .await
            }
            _ => unreachable!(),
        };
        let after_elig = after
            .check_eligibility(mercenary, &constraint, db.pool())
            .await;

        let blue_ids = g.nodes_by_team(Team::Blue);
        let blue_casualties = blue_ids
            .iter()
            .filter(|&&id| before.diversity(id) >= 2 && after.diversity(id) < 2)
            .count();

        table.add(MechanismComparison {
            scenario: "mercenary_bot".to_string(),
            mechanism: mechanism_name.to_string(),
            target_name: "mercenary".to_string(),
            before_distance: before.distance(mercenary),
            before_diversity: before.diversity(mercenary),
            before_eligible: before_elig.is_eligible,
            after_distance: after.distance(mercenary),
            after_diversity: after.diversity(mercenary),
            after_eligible: after_elig.is_eligible,
            blue_casualties,
            blue_total: blue_ids.len(),
            survived_weaponization: None,
        });
    }

    // Print comparison table
    eprintln!("\n=== Mechanism Comparison ===\n{table}");

    // Write to file
    table
        .write_to(std::path::Path::new(
            "target/simulation/mechanism_comparison.txt",
        ))
        .expect("write comparison table");
}
```

Note: The blue_casualties counting above uses diversity as a proxy (checking report data rather than async eligibility checks in a filter closure). This is simpler and sufficient — if diversity drops below 2, CommunityConstraint(5.0, 2) will reject.

**Step 3: Run the test**

Run: `cargo test --test trust_simulation_tests sim_mechanism_comparison -- --nocapture`
Expected: PASS — table printed to stderr and written to file

**Step 4: Commit**

```bash
git add service/tests/trust_simulation_tests.rs
git commit -m "test: add mechanism comparison test for hub-and-spoke and mercenary bot"
```

---

### Task 13: Add weaponization resistance test

**Files:**
- Modify: `service/tests/trust_simulation_tests.rs`

**Step 1: Add the weaponization scenario**

This tests whether Sybil nodes can abuse denouncement to remove a legitimate user:

```rust
// ---------------------------------------------------------------------------
// Weaponization test: Sybil cluster mass-denounces a legitimate user
//
// Topology:
//   anchor (blue) → 3 blue bridges → blue_target (well-connected)
//   red hub → 5 red spokes (Sybil cluster, diversity=1 each)
//   Each red node files d=2 denouncements against blue_target
//
// Question: does each mechanism protect blue_target from mass denouncement?
// Edge removal: would remove blue_target's edges (BAD — weaponizable)
// Score penalty: would degrade blue_target's score (BAD if penalties stack)
// Sponsorship cascade: would penalize blue_target's endorsers (BAD)
// ---------------------------------------------------------------------------
#[shared_runtime_test]
async fn sim_weaponization_resistance() {
    let mut table = ComparisonTable::new();

    for mechanism_name in &["edge_removal", "score_penalty", "sponsorship_cascade"] {
        let db = isolated_db().await;
        let mut g = GraphBuilder::new(db.pool().clone());

        // Blue team: well-connected target
        let anchor = g.add_node("anchor", Team::Blue).await;
        let bridge_a = g.add_node("bridge_a", Team::Blue).await;
        let bridge_b = g.add_node("bridge_b", Team::Blue).await;
        let bridge_c = g.add_node("bridge_c", Team::Blue).await;
        let blue_target = g.add_node("blue_target", Team::Blue).await;
        g.endorse(anchor, bridge_a, 1.0).await;
        g.endorse(anchor, bridge_b, 1.0).await;
        g.endorse(anchor, bridge_c, 1.0).await;
        g.endorse(bridge_a, blue_target, 1.0).await;
        g.endorse(bridge_b, blue_target, 1.0).await;
        g.endorse(bridge_c, blue_target, 1.0).await;

        // Red team: Sybil cluster (irrelevant to topology, but they "denounce")
        let _red_hub = g.add_node("red_hub", Team::Red).await;
        for i in 0..5 {
            let _spoke = g.add_node(&format!("red_spoke_{i}"), Team::Red).await;
        }

        // Before: blue_target should be eligible
        let before = SimulationReport::run(&g, anchor).await;
        before.materialize(db.pool()).await;
        let constraint = CommunityConstraint::new(5.0, 2).expect("valid");
        let before_elig = before
            .check_eligibility(blue_target, &constraint, db.pool())
            .await;
        assert!(before_elig.is_eligible, "blue_target should start eligible");

        // Simulate denouncement effect on blue_target
        // (In reality, denouncements come from red nodes, but the mechanism
        //  applies to the target regardless of who denounced)
        let after = match *mechanism_name {
            "edge_removal" => {
                mechanisms::apply_edge_removal(&mut g, blue_target, anchor, db.pool())
                    .await
            }
            "score_penalty" => {
                // 5 denouncers × 2 denouncements each = 10 denouncements
                // Each adds distance 3.0 and removes diversity 1
                mechanisms::apply_score_penalty(
                    &g,
                    blue_target,
                    anchor,
                    db.pool(),
                    30.0, // 10 × 3.0
                    10,   // 10 × 1
                )
                .await
            }
            "sponsorship_cascade" => {
                mechanisms::apply_sponsorship_cascade(
                    &mut g,
                    blue_target,
                    anchor,
                    db.pool(),
                )
                .await
            }
            _ => unreachable!(),
        };
        let after_elig = after
            .check_eligibility(blue_target, &constraint, db.pool())
            .await;

        let survived = after_elig.is_eligible;

        let blue_ids = g.nodes_by_team(Team::Blue);
        let blue_casualties = blue_ids
            .iter()
            .filter(|&&id| id != blue_target && before.diversity(id) >= 2 && after.diversity(id) < 2)
            .count();

        table.add(MechanismComparison {
            scenario: "weaponization".to_string(),
            mechanism: mechanism_name.to_string(),
            target_name: "blue_target".to_string(),
            before_distance: before.distance(blue_target),
            before_diversity: before.diversity(blue_target),
            before_eligible: before_elig.is_eligible,
            after_distance: after.distance(blue_target),
            after_diversity: after.diversity(blue_target),
            after_eligible: after_elig.is_eligible,
            blue_casualties,
            blue_total: blue_ids.len() - 1, // exclude target
            survived_weaponization: Some(survived),
        });
    }

    eprintln!("\n=== Weaponization Resistance ===\n{table}");

    table
        .write_to(std::path::Path::new(
            "target/simulation/weaponization_resistance.txt",
        ))
        .expect("write weaponization table");
}
```

**Step 2: Run the test**

Run: `cargo test --test trust_simulation_tests sim_weaponization_resistance -- --nocapture`
Expected: PASS — table shows which mechanisms protect/fail the blue target

**Step 3: Commit**

```bash
git add service/tests/trust_simulation_tests.rs
git commit -m "test: add weaponization resistance scenario"
```

---

### Task 14: Final verification

**Step 1: Run all simulation tests**

Run: `cargo test --test trust_simulation_tests -- --nocapture`
Expected: All tests pass (original 6 + new 6 scenarios + 2 comparison tests = 14 total)

**Step 2: Run all trust tests**

Run: `cargo test --test trust_simulation_tests --test trust_engine_tests --test trust_sybil_tests --test trust_e2e_tests --test trust_constraint_tests -- --nocapture`
Expected: All pass

**Step 3: Run lint**

Run: `just lint-backend`
Expected: Clean

**Step 4: Push**

```bash
git push origin test/624-trust-simulation-harness
```
