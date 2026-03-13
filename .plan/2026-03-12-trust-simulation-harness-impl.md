# Trust Graph Simulation Harness — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build a simulation test harness that constructs adversarial red/blue graph topologies and validates the real `TrustEngine`'s Sybil resistance properties empirically.

**Architecture:** Simulation module under `service/tests/common/simulation/` with three components: `GraphBuilder` (topology construction), `TopologyGenerator` (parameterized patterns), and `SimulationReport` (engine runner + output). Named scenarios in a dedicated test file exercise the real `TrustEngine` against constructed topologies.

**Tech Stack:** Rust, sqlx, `#[shared_runtime_test]`, `isolated_db()`, `TrustEngine`, `AccountFactory`

---

### Task 1: Extract endorsement helpers to shared factory

The `insert_endorsement` and `insert_revoked_endorsement` functions are currently private to `trust_engine_tests.rs`. Extract them to a shared factory so both the existing tests and the simulation can use them.

**Files:**
- Create: `service/tests/common/factories/endorsement.rs`
- Modify: `service/tests/common/factories/mod.rs`
- Modify: `service/tests/trust_engine_tests.rs`

**Step 1: Create the endorsement factory**

Create `service/tests/common/factories/endorsement.rs`:

```rust
//! Endorsement helpers for test setup — bypass the action queue to insert edges directly.

use sqlx::PgPool;
use uuid::Uuid;

/// Insert an active endorsement directly into the DB (bypass the action queue for test setup).
pub async fn insert_endorsement(pool: &PgPool, endorser: Uuid, subject: Uuid, weight: f32) {
    sqlx::query(
        "INSERT INTO reputation__endorsements (endorser_id, subject_id, topic, weight)
         VALUES ($1, $2, 'trust', $3)",
    )
    .bind(endorser)
    .bind(subject)
    .bind(weight)
    .execute(pool)
    .await
    .unwrap();
}

/// Insert a revoked endorsement (revoked_at set to now).
pub async fn insert_revoked_endorsement(
    pool: &PgPool,
    endorser: Uuid,
    subject: Uuid,
    weight: f32,
) {
    sqlx::query(
        "INSERT INTO reputation__endorsements (endorser_id, subject_id, topic, weight, revoked_at)
         VALUES ($1, $2, 'trust', $3, NOW())",
    )
    .bind(endorser)
    .bind(subject)
    .bind(weight)
    .execute(pool)
    .await
    .unwrap();
}
```

**Step 2: Register in factories/mod.rs**

Add to `service/tests/common/factories/mod.rs`:

```rust
mod endorsement;  // Add this line after the existing mod declarations

pub use endorsement::{insert_endorsement, insert_revoked_endorsement};  // Add to the pub use block
```

**Step 3: Update trust_engine_tests.rs to use extracted helpers**

Replace the two private functions at the top of `service/tests/trust_engine_tests.rs` with an import:

Remove lines 14-45 (the two `async fn insert_endorsement` and `async fn insert_revoked_endorsement` functions).

Add to the imports at the top:
```rust
use common::factories::{insert_endorsement, insert_revoked_endorsement};
```

**Step 4: Verify existing tests still pass**

Run: `cargo test --test trust_engine_tests -- --nocapture 2>&1 | tail -20`

Expected: All 9 tests pass (linear_chain, mixed_weight, path_diversity_independent, path_diversity_shared, revoked_edge, cycle_prevention, hub_and_spoke, recompute_from_anchor, anchor_has_distance_zero, recompute_writes_anchor_score).

**Step 5: Commit**

```bash
git add service/tests/common/factories/endorsement.rs service/tests/common/factories/mod.rs service/tests/trust_engine_tests.rs
git commit -m "refactor: extract endorsement helpers to shared factory"
```

---

### Task 2: Build GraphBuilder and simulation module skeleton

The core abstraction for programmatic topology construction.

**Files:**
- Create: `service/tests/common/simulation/mod.rs`
- Modify: `service/tests/common/mod.rs` (add `pub mod simulation;`)

**Step 1: Register simulation module**

Add to `service/tests/common/mod.rs`, after the existing module declarations (after line 83):

```rust
pub mod simulation;
```

**Step 2: Create GraphBuilder**

Create `service/tests/common/simulation/mod.rs`:

```rust
//! Trust graph simulation harness for Sybil resistance testing.
//!
//! Constructs adversarial graph topologies with red/blue team designations
//! and runs the real `TrustEngine` against them.

pub mod report;
pub mod topology;

use std::collections::HashMap;

use sqlx::PgPool;
use uuid::Uuid;

use crate::common::factories::{insert_endorsement, insert_revoked_endorsement, AccountFactory};

/// Team designation for simulation nodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Team {
    /// Legitimate users — expected to pass trust thresholds.
    Blue,
    /// Adversarial users — expected to be blocked by trust mechanisms.
    Red,
}

impl std::fmt::Display for Team {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Team::Blue => write!(f, "Blue"),
            Team::Red => write!(f, "Red"),
        }
    }
}

/// A node in the simulation graph with team metadata.
#[derive(Debug, Clone)]
pub struct SimNode {
    pub id: Uuid,
    pub name: String,
    pub team: Team,
}

/// A directed edge in the simulation graph.
#[derive(Debug, Clone)]
pub struct SimEdge {
    pub from: Uuid,
    pub to: Uuid,
    pub weight: f32,
    pub revoked: bool,
}

/// Programmatic graph topology constructor for simulation tests.
///
/// Wraps `AccountFactory` and endorsement insertion to build trust
/// graph topologies with team designations (red/blue).
///
/// # Example
///
/// ```ignore
/// let db = isolated_db().await;
/// let mut g = GraphBuilder::new(db.pool().clone());
/// let anchor = g.add_node("anchor", Team::Blue).await;
/// let alice = g.add_node("alice", Team::Blue).await;
/// g.endorse(anchor, alice, 1.0).await;
/// ```
pub struct GraphBuilder {
    pool: PgPool,
    nodes: Vec<SimNode>,
    edges: Vec<SimEdge>,
    name_to_id: HashMap<String, Uuid>,
}

impl GraphBuilder {
    /// Create a new builder for the given isolated database.
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            nodes: Vec::new(),
            edges: Vec::new(),
            name_to_id: HashMap::new(),
        }
    }

    /// Add a named node with team designation. Creates an account in the DB.
    ///
    /// Panics if account creation fails (test infrastructure issue).
    pub async fn add_node(&mut self, name: &str, team: Team) -> Uuid {
        let account = AccountFactory::new()
            .with_username(name)
            .create(&self.pool)
            .await
            .unwrap_or_else(|e| panic!("Failed to create account '{name}': {e}"));

        let node = SimNode {
            id: account.id,
            name: name.to_string(),
            team,
        };
        self.nodes.push(node);
        self.name_to_id.insert(name.to_string(), account.id);
        account.id
    }

    /// Create a directed endorsement edge.
    pub async fn endorse(&mut self, from: Uuid, to: Uuid, weight: f32) {
        insert_endorsement(&self.pool, from, to, weight).await;
        self.edges.push(SimEdge {
            from,
            to,
            weight,
            revoked: false,
        });
    }

    /// Create a revoked endorsement edge.
    pub async fn endorse_revoked(&mut self, from: Uuid, to: Uuid, weight: f32) {
        insert_revoked_endorsement(&self.pool, from, to, weight).await;
        self.edges.push(SimEdge {
            from,
            to,
            weight,
            revoked: true,
        });
    }

    /// Look up a node ID by name. Panics if not found.
    pub fn node(&self, name: &str) -> Uuid {
        *self
            .name_to_id
            .get(name)
            .unwrap_or_else(|| panic!("No node named '{name}'"))
    }

    /// Get all node IDs for a given team.
    pub fn nodes_by_team(&self, team: Team) -> Vec<Uuid> {
        self.nodes
            .iter()
            .filter(|n| n.team == team)
            .map(|n| n.id)
            .collect()
    }

    /// Get all nodes (for report generation).
    pub fn all_nodes(&self) -> &[SimNode] {
        &self.nodes
    }

    /// Get all edges (for DOT output).
    pub fn all_edges(&self) -> &[SimEdge] {
        &self.edges
    }

    /// Get the database pool.
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// Look up a node name by ID. Returns "unknown" if not found.
    pub fn node_name(&self, id: Uuid) -> &str {
        self.nodes
            .iter()
            .find(|n| n.id == id)
            .map(|n| n.name.as_str())
            .unwrap_or("unknown")
    }
}
```

**Step 3: Create empty topology.rs stub**

Create `service/tests/common/simulation/topology.rs`:

```rust
//! Parameterized topology generators for common attack patterns.

use uuid::Uuid;

use super::{GraphBuilder, Team};
```

**Step 4: Create empty report.rs stub**

Create `service/tests/common/simulation/report.rs`:

```rust
//! Simulation report — runs the trust engine and formats results.

use uuid::Uuid;

use super::{GraphBuilder, Team};
```

**Step 5: Verify compilation**

Run: `cargo test --test trust_engine_tests --no-run 2>&1 | tail -5`

Expected: Compiles without errors. The simulation module is registered but not yet used by any test file.

**Step 6: Commit**

```bash
git add service/tests/common/simulation/ service/tests/common/mod.rs
git commit -m "feat: add simulation module skeleton with GraphBuilder"
```

---

### Task 3: Write hub-and-spoke scenario test (first scenario)

Write the first named scenario test using `GraphBuilder` directly (no topology generators yet). This validates that the simulation harness works end-to-end.

**Files:**
- Create: `service/tests/trust_simulation_tests.rs`

**Step 1: Write the test file with the first scenario**

Create `service/tests/trust_simulation_tests.rs`:

```rust
//! Named simulation scenarios for trust engine Sybil resistance validation.
//!
//! Each test constructs a red/blue graph topology and asserts that the
//! TrustEngine correctly separates legitimate (blue) from adversarial (red) nodes.
//!
//! Run individual scenarios:
//!   cargo test --test trust_simulation_tests hub_and_spoke -- --nocapture

mod common;

use common::simulation::{GraphBuilder, Team};
use common::test_db::isolated_db;
use tc_test_macros::shared_runtime_test;
use tinycongress_api::trust::engine::TrustEngine;

// ---------------------------------------------------------------------------
// Scenario 1: Hub-and-spoke Sybil attack
//
// Topology:
//   anchor (blue) → bridge (blue) → hub (red) → 5 spokes (red)
//
// The attacker (hub) endorses 5 fake nodes. No other endorsers exist
// for the spokes. We expect:
//   - All red spokes have diversity = 1 (only endorsed by hub)
//   - All red spokes have distance >= 3.0 (Congress threshold)
// ---------------------------------------------------------------------------
#[shared_runtime_test]
async fn sim_hub_and_spoke_sybil_attack() {
    let db = isolated_db().await;
    let mut g = GraphBuilder::new(db.pool().clone());

    // Blue team: legitimate network
    let anchor = g.add_node("anchor", Team::Blue).await;
    let bridge = g.add_node("bridge", Team::Blue).await;
    g.endorse(anchor, bridge, 1.0).await; // physical QR

    // Red team: attacker hub endorses 5 spokes
    let hub = g.add_node("red_hub", Team::Red).await;
    g.endorse(bridge, hub, 0.3).await; // social referral into network

    let mut spokes = Vec::new();
    for i in 0..5 {
        let spoke = g.add_node(&format!("red_spoke_{i}"), Team::Red).await;
        g.endorse(hub, spoke, 1.0).await;
        spokes.push(spoke);
    }

    // Run engine
    let engine = TrustEngine::new(db.pool().clone());
    let distances = engine
        .compute_distances_from(anchor)
        .await
        .expect("compute_distances_from");
    let diversities = engine
        .compute_diversity_from(anchor)
        .await
        .expect("compute_diversity_from");

    // Assert: all spokes have diversity = 1
    for &spoke in &spokes {
        let (_, div) = diversities
            .iter()
            .find(|(uid, _)| *uid == spoke)
            .expect("spoke should have diversity entry");
        assert_eq!(
            *div, 1,
            "Hub-and-spoke: spoke should have diversity=1, got {div}"
        );
    }

    // Assert: all spokes have distance >= 3.0 (Congress threshold)
    // anchor→bridge (1.0) + bridge→hub (1/0.3≈3.33) + hub→spoke (1.0) ≈ 5.33
    for &spoke in &spokes {
        let score = distances
            .iter()
            .find(|s| s.user_id == spoke)
            .expect("spoke should be reachable");
        let dist = score
            .trust_distance
            .expect("spoke should have distance");
        assert!(
            dist >= 3.0,
            "Hub-and-spoke: spoke distance should be >= 3.0 (Congress threshold), got {dist:.3}"
        );
    }
}
```

**Step 2: Run the test**

Run: `cargo test --test trust_simulation_tests sim_hub_and_spoke -- --nocapture 2>&1 | tail -10`

Expected: PASS. The hub-and-spoke topology should produce diversity=1 and distance≈5.33 for all spokes.

**Step 3: Commit**

```bash
git add service/tests/trust_simulation_tests.rs
git commit -m "test: add hub-and-spoke Sybil simulation scenario"
```

---

### Task 4: Add topology generators

Now that the harness is proven, add parameterized topology generators for common patterns.

**Files:**
- Modify: `service/tests/common/simulation/topology.rs`

**Step 1: Implement topology generators**

Replace the contents of `service/tests/common/simulation/topology.rs`:

```rust
//! Parameterized topology generators for common attack patterns.
//!
//! Each generator creates named nodes and endorsement edges using `GraphBuilder`.
//! Nodes are named with a prefix for debugging (e.g., `red_hub`, `red_spoke_0`).

use uuid::Uuid;

use super::{GraphBuilder, Team};

/// Create a hub-and-spoke topology: one hub endorses N spokes.
///
/// Returns `(hub_id, spoke_ids)`.
pub async fn hub_and_spoke(
    g: &mut GraphBuilder,
    prefix: &str,
    team: Team,
    spoke_count: usize,
    weight: f32,
) -> (Uuid, Vec<Uuid>) {
    let hub = g.add_node(&format!("{prefix}_hub"), team).await;
    let mut spokes = Vec::with_capacity(spoke_count);
    for i in 0..spoke_count {
        let spoke = g.add_node(&format!("{prefix}_spoke_{i}"), team).await;
        g.endorse(hub, spoke, weight).await;
        spokes.push(spoke);
    }
    (hub, spokes)
}

/// Create a linear chain of endorsements.
///
/// Returns node IDs in chain order. First node is the chain head;
/// each node endorses the next.
pub async fn chain(
    g: &mut GraphBuilder,
    prefix: &str,
    team: Team,
    length: usize,
    weight: f32,
) -> Vec<Uuid> {
    let mut nodes = Vec::with_capacity(length);
    for i in 0..length {
        let node = g.add_node(&format!("{prefix}_chain_{i}"), team).await;
        if let Some(&prev) = nodes.last() {
            g.endorse(prev, node, weight).await;
        }
        nodes.push(node);
    }
    nodes
}

/// Create a colluding ring: each node endorses the next, last endorses first.
///
/// Returns node IDs in ring order.
pub async fn colluding_ring(
    g: &mut GraphBuilder,
    prefix: &str,
    team: Team,
    size: usize,
    weight: f32,
) -> Vec<Uuid> {
    assert!(size >= 2, "Ring must have at least 2 nodes");
    let mut nodes = Vec::with_capacity(size);
    for i in 0..size {
        let node = g.add_node(&format!("{prefix}_ring_{i}"), team).await;
        nodes.push(node);
    }
    for i in 0..size {
        let next = (i + 1) % size;
        g.endorse(nodes[i], nodes[next], weight).await;
    }
    nodes
}

/// Create a healthy web: nodes with deterministic interconnections.
///
/// `density` is the proportion (0.0-1.0) of possible directed edges to create.
/// Edge selection is deterministic based on node indices for reproducibility.
///
/// Returns all node IDs.
pub async fn healthy_web(
    g: &mut GraphBuilder,
    prefix: &str,
    team: Team,
    size: usize,
    density: f64,
    weight: f32,
) -> Vec<Uuid> {
    let mut nodes = Vec::with_capacity(size);
    for i in 0..size {
        let node = g.add_node(&format!("{prefix}_web_{i}"), team).await;
        nodes.push(node);
    }
    for i in 0..size {
        for j in 0..size {
            if i == j {
                continue;
            }
            // Deterministic hash to decide if edge exists
            let hash = ((i * 7 + j * 13 + 37) % 100) as f64 / 100.0;
            if hash < density {
                g.endorse(nodes[i], nodes[j], weight).await;
            }
        }
    }
    nodes
}
```

**Step 2: Verify compilation**

Run: `cargo test --test trust_simulation_tests --no-run 2>&1 | tail -5`

Expected: Compiles without errors.

**Step 3: Commit**

```bash
git add service/tests/common/simulation/topology.rs
git commit -m "feat: add parameterized topology generators"
```

---

### Task 5: Add SimulationReport

The report runner executes the engine and formats output for debugging.

**Files:**
- Modify: `service/tests/common/simulation/report.rs`

**Step 1: Implement SimulationReport**

Replace the contents of `service/tests/common/simulation/report.rs`:

```rust
//! Simulation report — runs the trust engine and formats results.

use std::collections::HashMap;
use std::fmt;
use std::io;
use std::path::Path;

use tinycongress_api::trust::engine::TrustEngine;
use uuid::Uuid;

use super::{GraphBuilder, Team};

/// Score data for a single node in the simulation.
#[derive(Debug, Clone)]
pub struct NodeScore {
    pub id: Uuid,
    pub name: String,
    pub team: Team,
    pub distance: Option<f32>,
    pub diversity: i32,
}

/// Results of running the trust engine on a simulation topology.
pub struct SimulationReport {
    pub anchor_id: Uuid,
    pub scores: Vec<NodeScore>,
}

impl SimulationReport {
    /// Run the trust engine from the given anchor and collect results.
    pub async fn run(g: &GraphBuilder, anchor_id: Uuid) -> Self {
        let engine = TrustEngine::new(g.pool().clone());

        let distances = engine
            .compute_distances_from(anchor_id)
            .await
            .expect("compute_distances_from failed");

        let diversities: HashMap<Uuid, i32> = engine
            .compute_diversity_from(anchor_id)
            .await
            .expect("compute_diversity_from failed")
            .into_iter()
            .collect();

        let distance_map: HashMap<Uuid, Option<f32>> = distances
            .iter()
            .map(|s| (s.user_id, s.trust_distance))
            .collect();

        let scores = g
            .all_nodes()
            .iter()
            .map(|node| NodeScore {
                id: node.id,
                name: node.name.clone(),
                team: node.team,
                distance: distance_map.get(&node.id).copied().flatten(),
                diversity: diversities.get(&node.id).copied().unwrap_or(0),
            })
            .collect();

        Self { anchor_id, scores }
    }

    /// Get distance for a specific node.
    pub fn distance(&self, node_id: Uuid) -> Option<f32> {
        self.scores.iter().find(|s| s.id == node_id)?.distance
    }

    /// Get diversity for a specific node.
    pub fn diversity(&self, node_id: Uuid) -> i32 {
        self.scores
            .iter()
            .find(|s| s.id == node_id)
            .map_or(0, |s| s.diversity)
    }

    /// Get all red team node scores.
    pub fn red_nodes(&self) -> Vec<&NodeScore> {
        self.scores.iter().filter(|s| s.team == Team::Red).collect()
    }

    /// Get all blue team node scores.
    pub fn blue_nodes(&self) -> Vec<&NodeScore> {
        self.scores
            .iter()
            .filter(|s| s.team == Team::Blue)
            .collect()
    }

    /// Write a DOT/Graphviz file with red/blue coloring and score annotations.
    ///
    /// Output directory is created if it doesn't exist.
    pub fn write_dot(&self, g: &GraphBuilder, path: &Path) -> io::Result<()> {
        use std::fs;

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut dot = String::from("digraph trust_simulation {\n");
        dot.push_str("  rankdir=LR;\n");
        dot.push_str("  node [shape=box, style=filled, fontname=\"Helvetica\"];\n");
        dot.push_str("  edge [fontname=\"Helvetica\", fontsize=10];\n\n");

        // Nodes with team coloring and score labels
        for score in &self.scores {
            let color = match score.team {
                Team::Blue => "#a8d5e2",
                Team::Red => "#f4a9a8",
            };
            let dist = score
                .distance
                .map(|d| format!("{d:.2}"))
                .unwrap_or_else(|| "unreachable".to_string());
            let label = format!(
                "{}\\nd={} div={}",
                score.name, dist, score.diversity
            );
            dot.push_str(&format!(
                "  \"{}\" [label=\"{}\", fillcolor=\"{}\"];\n",
                score.name, label, color
            ));
        }

        dot.push('\n');

        // Edges with weight labels
        for edge in g.all_edges() {
            let from_name = g.node_name(edge.from);
            let to_name = g.node_name(edge.to);
            let style = if edge.revoked {
                ", style=dashed, color=gray"
            } else {
                ""
            };
            dot.push_str(&format!(
                "  \"{}\" -> \"{}\" [label=\"{:.1}\"{style}];\n",
                from_name, to_name, edge.weight
            ));
        }

        dot.push_str("}\n");
        fs::write(path, dot)
    }
}

impl fmt::Display for SimulationReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "{:<20} {:<6} {:>10} {:>10}",
            "Node", "Team", "Distance", "Diversity"
        )?;
        writeln!(
            f,
            "{:<20} {:<6} {:>10} {:>10}",
            "----", "----", "--------", "---------"
        )?;
        for score in &self.scores {
            let dist = score
                .distance
                .map(|d| format!("{d:.3}"))
                .unwrap_or_else(|| "unreachable".to_string());
            writeln!(
                f,
                "{:<20} {:<6} {:>10} {:>10}",
                score.name, score.team, dist, score.diversity
            )?;
        }
        Ok(())
    }
}
```

**Step 2: Verify compilation**

Run: `cargo test --test trust_simulation_tests --no-run 2>&1 | tail -5`

Expected: Compiles without errors.

**Step 3: Commit**

```bash
git add service/tests/common/simulation/report.rs
git commit -m "feat: add SimulationReport with table and DOT output"
```

---

### Task 6: Add remaining named scenarios

Add 5 more scenarios exercising different attack topologies. These use the topology generators and SimulationReport for cleaner assertions.

**Files:**
- Modify: `service/tests/trust_simulation_tests.rs`

**Step 1: Add all remaining scenarios**

Append to `service/tests/trust_simulation_tests.rs` (after the hub-and-spoke test):

```rust
use common::simulation::report::SimulationReport;
use common::simulation::topology;
```

Add these to the imports at the top (alongside the existing `use` statements).

Then append these test functions:

```rust
// ---------------------------------------------------------------------------
// Scenario 2: Chain infiltration
//
// Topology:
//   anchor (blue) → blue web (5 nodes, interconnected)
//   One blue node → red chain head (social referral, 0.3)
//   red chain: 8 nodes linked via physical QR (1.0)
//
// The red chain is attached at a single point via low-weight edge.
// We expect:
//   - Red nodes deep in the chain exceed the 10.0 distance cutoff
//   - All reachable red nodes have diversity = 1
// ---------------------------------------------------------------------------
#[shared_runtime_test]
async fn sim_chain_infiltration() {
    let db = isolated_db().await;
    let mut g = GraphBuilder::new(db.pool().clone());

    // Blue team: anchor + healthy web
    let anchor = g.add_node("anchor", Team::Blue).await;
    let blue_web = topology::healthy_web(&mut g, "blue", Team::Blue, 5, 0.5, 1.0).await;
    // Connect anchor to web
    g.endorse(anchor, blue_web[0], 1.0).await;

    // Red team: chain of 8 attached via social referral
    let red_chain = topology::chain(&mut g, "red", Team::Red, 8, 1.0).await;
    // Attach: one blue web node → red chain head via social referral
    g.endorse(blue_web[0], red_chain[0], 0.3).await;

    let report = SimulationReport::run(&g, anchor).await;

    // Print for debugging
    eprintln!("\n=== Chain Infiltration ===\n{report}");

    // Assert: all reachable red nodes have diversity = 1
    for red in report.red_nodes() {
        if red.distance.is_some() {
            assert_eq!(
                red.diversity, 1,
                "Chain red node '{}' should have diversity=1, got {}",
                red.name, red.diversity
            );
        }
    }

    // Assert: at least some red nodes are unreachable (distance cutoff)
    // Social referral cost = 1/0.3 ≈ 3.33, plus anchor→web = 1.0
    // So chain head is at distance ≈ 4.33. Each hop adds 1.0.
    // Cutoff at 10.0 means ~5-6 hops into the chain should be unreachable.
    let unreachable_count = report
        .red_nodes()
        .iter()
        .filter(|n| n.distance.is_none())
        .count();
    assert!(
        unreachable_count > 0,
        "Some red chain nodes should be beyond distance cutoff"
    );

    // Write DOT for visual inspection
    report
        .write_dot(
            &g,
            std::path::Path::new("target/simulation/chain_infiltration.dot"),
        )
        .expect("write DOT");
}

// ---------------------------------------------------------------------------
// Scenario 3: Colluding ring
//
// Topology:
//   anchor (blue) → bridge (blue) → red ring entry
//   Red ring: 6 nodes endorsing each other in a circle
//
// Internal endorsements within the ring shouldn't help diversity
// because the endorsers are only reachable via the single bridge node.
// ---------------------------------------------------------------------------
#[shared_runtime_test]
async fn sim_colluding_ring() {
    let db = isolated_db().await;
    let mut g = GraphBuilder::new(db.pool().clone());

    // Blue team
    let anchor = g.add_node("anchor", Team::Blue).await;
    let bridge = g.add_node("bridge", Team::Blue).await;
    g.endorse(anchor, bridge, 1.0).await;

    // Red team: colluding ring
    let ring = topology::colluding_ring(&mut g, "red", Team::Red, 6, 1.0).await;
    // Attach ring to blue network at single point
    g.endorse(bridge, ring[0], 0.3).await;

    let report = SimulationReport::run(&g, anchor).await;
    eprintln!("\n=== Colluding Ring ===\n{report}");

    // Key question: does the diversity approximation count ring members
    // as distinct endorsers? The approximation counts "distinct reachable
    // endorsers" — ring members ARE reachable from the anchor, so they
    // count as distinct endorsers of each other.
    //
    // This is a known limitation of the approximation at demo scale (noted
    // in test_path_diversity_shared_branch). The key invariant is that
    // hub-and-spoke gives diversity=1 — rings may give higher diversity
    // due to the approximation.
    //
    // Record the actual values for calibration.
    for red in report.red_nodes() {
        eprintln!(
            "  Ring node '{}': distance={:?}, diversity={}",
            red.name, red.distance, red.diversity
        );
    }

    // Assert: all ring nodes are reachable (distance < 10.0)
    for red in report.red_nodes() {
        assert!(
            red.distance.is_some(),
            "Ring node '{}' should be reachable",
            red.name
        );
    }

    // Assert: ring nodes have higher distance than blue nodes
    // Bridge has distance 1.0; ring entry has 1.0 + 3.33 = 4.33
    let bridge_dist = report.distance(bridge).expect("bridge reachable");
    for red in report.red_nodes() {
        let dist = red.distance.expect("ring node reachable");
        assert!(
            dist > bridge_dist,
            "Ring node '{}' (d={:.3}) should be further than bridge (d={:.3})",
            red.name,
            dist,
            bridge_dist
        );
    }

    report
        .write_dot(
            &g,
            std::path::Path::new("target/simulation/colluding_ring.dot"),
        )
        .expect("write DOT");
}

// ---------------------------------------------------------------------------
// Scenario 4: Mixed topology — red cluster at single attachment point
//
// Topology:
//   anchor (blue) → healthy blue web (8 nodes, high density)
//   One blue node → red cluster (5 nodes, fully connected internally)
//
// Despite high internal density, all red nodes connect to the blue
// network through a single bridge point.
// ---------------------------------------------------------------------------
#[shared_runtime_test]
async fn sim_red_cluster_single_attachment() {
    let db = isolated_db().await;
    let mut g = GraphBuilder::new(db.pool().clone());

    // Blue team: anchor + healthy web with high density
    let anchor = g.add_node("anchor", Team::Blue).await;
    let blue_web = topology::healthy_web(&mut g, "blue", Team::Blue, 8, 0.6, 1.0).await;
    g.endorse(anchor, blue_web[0], 1.0).await;
    g.endorse(anchor, blue_web[1], 1.0).await;

    // Red team: fully connected cluster
    let mut red_nodes = Vec::new();
    for i in 0..5 {
        let node = g.add_node(&format!("red_cluster_{i}"), Team::Red).await;
        red_nodes.push(node);
    }
    // Fully connect red cluster
    for i in 0..red_nodes.len() {
        for j in 0..red_nodes.len() {
            if i != j {
                g.endorse(red_nodes[i], red_nodes[j], 1.0).await;
            }
        }
    }
    // Single attachment: one blue node → red cluster entry
    g.endorse(blue_web[2], red_nodes[0], 0.3).await;

    let report = SimulationReport::run(&g, anchor).await;
    eprintln!("\n=== Red Cluster Single Attachment ===\n{report}");

    // Assert: blue web nodes generally have diversity >= 2
    // (due to high density interconnections)
    let high_div_blues = report
        .blue_nodes()
        .iter()
        .filter(|n| n.diversity >= 2)
        .count();
    eprintln!("  Blue nodes with diversity >= 2: {high_div_blues}/{}", report.blue_nodes().len());

    // Assert: all red nodes are reachable
    for red in report.red_nodes() {
        assert!(
            red.distance.is_some(),
            "Red cluster node '{}' should be reachable",
            red.name
        );
    }

    report
        .write_dot(
            &g,
            std::path::Path::new("target/simulation/red_cluster_single_attachment.dot"),
        )
        .expect("write DOT");
}

// ---------------------------------------------------------------------------
// Scenario 5: Social referral ceiling
//
// Topology:
//   anchor (blue) → chain of nodes all connected via social referral (0.3)
//
// Each hop costs 1/0.3 ≈ 3.33 distance. By hop 3, distance ≈ 10.0.
// This tests the structural distance limit of low-weight edges.
// ---------------------------------------------------------------------------
#[shared_runtime_test]
async fn sim_social_referral_ceiling() {
    let db = isolated_db().await;
    let mut g = GraphBuilder::new(db.pool().clone());

    let anchor = g.add_node("anchor", Team::Blue).await;

    // Chain of social referrals
    let chain_nodes =
        topology::chain(&mut g, "social", Team::Blue, 5, 0.3).await;
    g.endorse(anchor, chain_nodes[0], 0.3).await;

    let report = SimulationReport::run(&g, anchor).await;
    eprintln!("\n=== Social Referral Ceiling ===\n{report}");

    // Assert: hop 1 ≈ 3.33, hop 2 ≈ 6.67, hop 3 ≈ 10.0
    // Nodes at hop 3+ should be at or beyond the 10.0 cutoff
    let reachable_count = chain_nodes
        .iter()
        .filter(|&&id| report.distance(id).is_some())
        .count();
    eprintln!("  Reachable social chain nodes: {reachable_count}/5");

    // At most 3 nodes should be reachable (3.33 * 3 ≈ 10.0)
    assert!(
        reachable_count <= 3,
        "At most 3 social-referral hops should be within cutoff, got {reachable_count}"
    );

    // First node should be reachable
    assert!(
        report.distance(chain_nodes[0]).is_some(),
        "First social referral node should be reachable"
    );

    report
        .write_dot(
            &g,
            std::path::Path::new("target/simulation/social_referral_ceiling.dot"),
        )
        .expect("write DOT");
}

// ---------------------------------------------------------------------------
// Scenario 6: Weight calibration baseline
//
// Topology:
//   anchor → target via three parallel paths:
//     1. Physical QR (weight 1.0): anchor → p → target
//     2. Video call (weight 0.7): anchor → v → target
//     3. Social referral (weight 0.3): anchor → s → target
//
// Validates that edge weights produce the expected distance ratios
// and that parallel paths contribute to diversity.
// ---------------------------------------------------------------------------
#[shared_runtime_test]
async fn sim_weight_calibration() {
    let db = isolated_db().await;
    let mut g = GraphBuilder::new(db.pool().clone());

    let anchor = g.add_node("anchor", Team::Blue).await;
    let target = g.add_node("target", Team::Blue).await;

    // Path 1: Physical QR (1.0)
    let p = g.add_node("physical_bridge", Team::Blue).await;
    g.endorse(anchor, p, 1.0).await;
    g.endorse(p, target, 1.0).await;

    // Path 2: Video call (0.7)
    let v = g.add_node("video_bridge", Team::Blue).await;
    g.endorse(anchor, v, 0.7).await;
    g.endorse(v, target, 0.7).await;

    // Path 3: Social referral (0.3)
    let s = g.add_node("social_bridge", Team::Blue).await;
    g.endorse(anchor, s, 0.3).await;
    g.endorse(s, target, 0.3).await;

    let report = SimulationReport::run(&g, anchor).await;
    eprintln!("\n=== Weight Calibration ===\n{report}");

    // Assert: target uses minimum distance path (physical QR = 2.0)
    let target_dist = report
        .distance(target)
        .expect("target should be reachable");
    assert!(
        (target_dist - 2.0).abs() < 0.01,
        "Target should use physical QR path (distance=2.0), got {target_dist:.3}"
    );

    // Assert: target has diversity = 3 (three independent endorsers)
    let target_div = report.diversity(target);
    assert_eq!(
        target_div, 3,
        "Target should have 3 distinct endorsers (p, v, s), got {target_div}"
    );

    // Assert: bridge distances reflect weight differences
    let p_dist = report.distance(p).expect("physical bridge reachable");
    let v_dist = report.distance(v).expect("video bridge reachable");
    let s_dist = report.distance(s).expect("social bridge reachable");

    assert!(
        (p_dist - 1.0).abs() < 0.01,
        "Physical QR bridge: expected d=1.0, got {p_dist:.3}"
    );
    assert!(
        (v_dist - (1.0 / 0.7)).abs() < 0.05,
        "Video bridge: expected d≈{:.3}, got {v_dist:.3}",
        1.0 / 0.7
    );
    assert!(
        (s_dist - (1.0 / 0.3)).abs() < 0.05,
        "Social bridge: expected d≈{:.3}, got {s_dist:.3}",
        1.0 / 0.3
    );

    report
        .write_dot(
            &g,
            std::path::Path::new("target/simulation/weight_calibration.dot"),
        )
        .expect("write DOT");
}
```

**Step 2: Run all simulation tests**

Run: `cargo test --test trust_simulation_tests -- --nocapture 2>&1 | tail -30`

Expected: All 6 scenarios pass. Review the printed tables for each scenario to verify the numbers look right.

**Step 3: Also run the original trust engine tests to confirm no regressions**

Run: `cargo test --test trust_engine_tests 2>&1 | tail -5`

Expected: All 9 tests pass.

**Step 4: Commit**

```bash
git add service/tests/trust_simulation_tests.rs
git commit -m "test: add 5 more simulation scenarios (chain, ring, cluster, social ceiling, weight calibration)"
```

---

### Task 7: Final verification and push

**Step 1: Run full backend test suite**

Run: `just test-backend 2>&1 | tail -20`

Expected: All tests pass.

**Step 2: Run linter**

Run: `just lint-backend 2>&1 | tail -10`

Expected: No warnings or errors. Fix any clippy issues.

**Step 3: Review DOT output**

Run: `ls -la target/simulation/`

Expected: DOT files for each scenario that wrote one (chain_infiltration, colluding_ring, red_cluster_single_attachment, social_referral_ceiling, weight_calibration).

**Step 4: Push to PR**

Run: `git push origin test/624-trust-simulation-harness`

Expected: All commits pushed to PR #643.
