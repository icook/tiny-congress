//! Named simulation scenarios for trust engine Sybil resistance validation.
//!
//! Each test constructs a red/blue graph topology and asserts that the
//! TrustEngine correctly separates legitimate (blue) from adversarial (red) nodes.
//!
//! Run individual scenarios:
//!   cargo test --test trust_simulation_tests hub_and_spoke -- --nocapture

mod common;

use common::simulation::comparison::{ComparisonTable, MechanismComparison};
use common::simulation::mechanisms;
use common::simulation::predicates;
use common::simulation::report::SimulationReport;
use common::simulation::{topology, GraphBuilder, GraphSpec, Team};
use common::test_db::isolated_db;
use tc_test_macros::shared_runtime_test;
use tinycongress_api::trust::constraints::{CommunityConstraint, CongressConstraint};
use uuid::Uuid;

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

    // Run engine via SimulationReport
    let report = SimulationReport::run(&g, anchor).await;

    // Assert: all spokes have diversity = 1
    for &spoke in &spokes {
        let div = report.diversity(spoke);
        assert_eq!(
            div, 1,
            "Hub-and-spoke: spoke should have diversity=1, got {div}"
        );
    }

    // Assert: all spokes have distance >= 3.0 (Congress threshold)
    // anchor→bridge (1.0) + bridge→hub (1/0.3≈3.33) + hub→spoke (1.0) ≈ 5.33
    // Observed: distance=5.333 for all spokes (matches expected math)
    for &spoke in &spokes {
        let dist = report.distance(spoke).expect("spoke should be reachable");
        assert!(
            dist >= 3.0,
            "Hub-and-spoke: spoke distance should be >= 3.0 (Congress threshold), got {dist:.3}"
        );
    }

    // Pipeline assertion: materialize scores, then verify spokes are
    // rejected by CommunityConstraint (diversity=1 < min_diversity=2).
    report.materialize(db.pool()).await;
    let constraint = CommunityConstraint::new(5.0, 2).expect("valid constraint");
    for &spoke in &spokes {
        let eligibility = report
            .check_eligibility(spoke, &constraint, db.pool())
            .await;
        assert!(
            !eligibility.is_eligible,
            "Hub-and-spoke: spoke should be rejected by CommunityConstraint(min_diversity=2)"
        );
    }
}

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

    // Blue team: anchor connected to all web nodes so the web is reachable
    let anchor = g.add_node("anchor", Team::Blue).await;
    let blue_web = topology::healthy_web(&mut g, "blue", Team::Blue, 5, 0.5, 1.0).await;
    for &web_node in &blue_web {
        g.endorse(anchor, web_node, 1.0).await;
    }

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

    // Vertex connectivity correctly identifies all ring nodes as diversity=1:
    // despite internal endorsements, the ring connects to the anchor through
    // a single bridge node (the only vertex-disjoint path).
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

    // Pipeline assertion: materialize scores, then verify ring nodes are
    // rejected by CongressConstraint (diversity=1 < min_diversity=2).
    report.materialize(db.pool()).await;
    let constraint = CongressConstraint::new(2).expect("valid constraint");
    for red in report.red_nodes() {
        let eligibility = report
            .check_eligibility(red.id, &constraint, db.pool())
            .await;
        assert!(
            !eligibility.is_eligible,
            "Colluding ring: node '{}' should be rejected by CongressConstraint(min_diversity=2), got eligible",
            red.name
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
    let high_div_blues = report
        .blue_nodes()
        .iter()
        .filter(|n| n.diversity >= 2)
        .count();
    eprintln!(
        "  Blue nodes with diversity >= 2: {high_div_blues}/{}",
        report.blue_nodes().len()
    );

    // Assert: all red nodes are reachable
    for red in report.red_nodes() {
        assert!(
            red.distance.is_some(),
            "Red cluster node '{}' should be reachable",
            red.name
        );
    }

    // Vertex connectivity correctly identifies all red cluster nodes as
    // diversity=1: despite being fully connected internally, the cluster
    // connects to the anchor through a single blue bridge node.
    for red in report.red_nodes() {
        eprintln!(
            "  Red cluster '{}': distance={:?}, diversity={}",
            red.name, red.distance, red.diversity
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
    let chain_nodes = topology::chain(&mut g, "social", Team::Blue, 5, 0.3).await;
    g.endorse(anchor, chain_nodes[0], 0.3).await;

    let report = SimulationReport::run(&g, anchor).await;
    eprintln!("\n=== Social Referral Ceiling ===\n{report}");

    // Distance per hop: 1/0.3 ≈ 3.33. The CTE checks `distance < 10.0`
    // before traversal, so a node AT 10.0 is included in results (it was
    // produced when the parent at 6.67 was traversed). Hop 4 at 13.33 is excluded.
    // Result: chain_nodes[0]=3.33, [1]=6.67, [2]=10.0 (included), [3]+ excluded.
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
    let target_dist = report.distance(target).expect("target should be reachable");
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

    // Pipeline assertion: materialize scores, then verify target passes
    // CommunityConstraint (distance=2.0 <= 5.0, diversity=3 >= 2).
    report.materialize(db.pool()).await;
    let constraint = CommunityConstraint::new(5.0, 2).expect("valid constraint");
    let eligibility = report
        .check_eligibility(target, &constraint, db.pool())
        .await;
    assert!(
        eligibility.is_eligible,
        "Weight calibration: target with distance=2.0 and diversity=3 should pass CommunityConstraint"
    );

    report
        .write_dot(
            &g,
            std::path::Path::new("target/simulation/weight_calibration.dot"),
        )
        .expect("write DOT");
}

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
    let red_cluster = topology::fully_connected_cluster(&mut g, "red", Team::Red, 5, 1.0).await;
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

    // Pipeline assertion: red nodes PASS CommunityConstraint(6.0, 2)
    // This is the dangerous case — adversaries that meet the threshold.
    // (max_distance=6.0 because the furthest red nodes are at distance ~5.33)
    report.materialize(db.pool()).await;
    let constraint = CommunityConstraint::new(6.0, 2).expect("valid constraint");
    for red in report.red_nodes() {
        let eligibility = report
            .check_eligibility(red.id, &constraint, db.pool())
            .await;
        assert!(
            eligibility.is_eligible,
            "Multi-point: red node '{}' should PASS CommunityConstraint(6.0, 2) — this is the attack succeeding",
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

// ---------------------------------------------------------------------------
// Scenario 8: Asymmetric weight exploitation
//
// Topology:
//   anchor (blue) → compromised_bridge (blue) at weight 1.0
//   compromised_bridge → red_node at weight 1.0 (maximum allowed)
//
// The red node is close (distance = 2.0) but has diversity=1.
// Tests that low distance cannot substitute for structural diversity.
// DB constraint limits weight to (0, 1.0], so the minimum cost per hop is 1.0.
// ---------------------------------------------------------------------------
#[shared_runtime_test]
async fn sim_asymmetric_weight_exploit() {
    let db = isolated_db().await;
    let mut g = GraphBuilder::new(db.pool().clone());

    let anchor = g.add_node("anchor", Team::Blue).await;
    let bridge = g.add_node("compromised_bridge", Team::Blue).await;
    g.endorse(anchor, bridge, 1.0).await;

    // Red node with max-weight endorsement. The DB enforces
    // CHECK (weight > 0 AND weight <= 1.0), so weight=1.0 IS the maximum.
    // The original design imagined testing weight=10.0 (cost=0.1, distance≈0.1)
    // to see if extreme closeness substitutes for diversity — but the DB cap
    // makes that impossible. The cap itself is the defense: no single endorsement
    // can push distance below cost=1.0. This test documents that at max weight
    // (cost=1.0), a single-path node still fails diversity constraints.
    let red = g.add_node("red_exploiter", Team::Red).await;
    g.endorse(bridge, red, 1.0).await; // cost = 1/1.0 = 1.0

    let report = SimulationReport::run(&g, anchor).await;
    eprintln!("\n=== Asymmetric Weight Exploit ===\n{report}");

    // Assert: red node distance = anchor→bridge (1.0) + bridge→red (1.0) = 2.0
    let red_dist = report.distance(red).expect("red should be reachable");
    assert!(
        (red_dist - 2.0).abs() < 0.01,
        "Asymmetric weight: red node should have distance=2.0, got {red_dist:.3}"
    );

    // Assert: diversity = 1 (only one path through compromised_bridge)
    let red_div = report.diversity(red);
    assert_eq!(
        red_div, 1,
        "Asymmetric weight: red node should have diversity=1, got {red_div}"
    );

    // Pipeline: even at max weight, diversity=1 < min=2 → rejected
    report.materialize(db.pool()).await;
    let community = CommunityConstraint::new(5.0, 2).expect("valid constraint");
    let eligibility = report.check_eligibility(red, &community, db.pool()).await;
    assert!(
        !eligibility.is_eligible,
        "Asymmetric weight: red node should fail CommunityConstraint (diversity=1 < min=2)"
    );

    report
        .write_dot(
            &g,
            std::path::Path::new("target/simulation/asymmetric_weight_exploit.dot"),
        )
        .expect("write DOT");
}

// ---------------------------------------------------------------------------
// Scenario 9: Phantom edges (near-zero weight)
//
// Topology:
//   anchor (blue) → bridge (blue) at weight 1.0
//   bridge → red_node at weight 0.001 (cost = 1000.0)
//
// An endorsement with near-zero weight creates a DB edge that is
// functionally nonexistent — the node IS technically reachable in the
// engine (distance ≈ 1001.0) but far beyond any useful threshold.
// This tests that distance-based constraints reject such phantom nodes.
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

    // Assert: red node is technically reachable but at extreme distance
    let red_dist = report
        .distance(red)
        .expect("phantom node should be reachable in engine");
    assert!(
        red_dist > 100.0,
        "Phantom edge: red node should have extreme distance (>100.0), got {red_dist:.3}"
    );

    // Assert: diversity = 1 (single path exists but is useless)
    assert_eq!(
        report.diversity(red),
        1,
        "Phantom edge: red node should have diversity=1"
    );

    // Pipeline: materialize and verify red is ineligible (distance far exceeds threshold)
    report.materialize(db.pool()).await;
    let constraint = CommunityConstraint::new(5.0, 2).expect("valid constraint");
    let eligibility = report.check_eligibility(red, &constraint, db.pool()).await;
    assert!(
        !eligibility.is_eligible,
        "Phantom edge: extreme-distance red node should fail CommunityConstraint"
    );

    report
        .write_dot(
            &g,
            std::path::Path::new("target/simulation/phantom_edges.dot"),
        )
        .expect("write DOT");
}

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
    assert!(
        a_before.is_eligible,
        "Before: downstream_a should be eligible (div=2)"
    );
    let b_before = report_before
        .check_eligibility(downstream_b, &constraint, db.pool())
        .await;
    assert!(
        !b_before.is_eligible,
        "Before: downstream_b should be ineligible (div=1)"
    );

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

    // Pipeline: materialize after revocation and verify eligibility changes
    report_after.materialize(db.pool()).await;
    let a_after = report_after
        .check_eligibility(downstream_a, &constraint, db.pool())
        .await;
    assert!(
        !a_after.is_eligible,
        "After: downstream_a should be ineligible (diversity dropped to 1, collateral damage)"
    );
    let b_after = report_after
        .check_eligibility(downstream_b, &constraint, db.pool())
        .await;
    assert!(
        !b_after.is_eligible,
        "After: downstream_b should be ineligible (unreachable)"
    );

    report_after
        .write_dot(
            &g,
            std::path::Path::new("target/simulation/graph_splitting.dot"),
        )
        .expect("write DOT");
}

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
    let coercer_dist = report
        .distance(coercer)
        .expect("coercer should be reachable");
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
        let before_elig = before.check_eligibility(hub, &constraint, db.pool()).await;

        // Apply mechanism to hub
        let after = match *mechanism_name {
            "edge_removal" => mechanisms::apply_edge_removal(&mut g, hub, anchor, db.pool()).await,
            "score_penalty" => {
                mechanisms::apply_score_penalty(&g, hub, anchor, db.pool(), 3.0, 1).await
            }
            "sponsorship_cascade" => {
                mechanisms::apply_sponsorship_cascade(&mut g, hub, anchor, db.pool()).await
            }
            _ => unreachable!(),
        };
        let after_elig = after.check_eligibility(hub, &constraint, db.pool()).await;

        // Count blue casualties
        let blue_ids: Vec<_> = g.nodes_by_team(Team::Blue);
        let blue_casualties = blue_ids
            .iter()
            .filter(|&&id| before.diversity(id) >= 2 && after.diversity(id) < 2)
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
                mechanisms::apply_score_penalty(&g, mercenary, anchor, db.pool(), 3.0, 1).await
            }
            "sponsorship_cascade" => {
                mechanisms::apply_sponsorship_cascade(&mut g, mercenary, anchor, db.pool()).await
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
        let after = match *mechanism_name {
            "edge_removal" => {
                mechanisms::apply_edge_removal(&mut g, blue_target, anchor, db.pool()).await
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
                mechanisms::apply_sponsorship_cascade(&mut g, blue_target, anchor, db.pool()).await
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
            .filter(|&&id| {
                id != blue_target && before.diversity(id) >= 2 && after.diversity(id) < 2
            })
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

// ---------------------------------------------------------------------------
// Predicate test 1: hub-and-spoke invariants
//
// Builds the same hub-and-spoke topology as Scenario 1 but asserts via
// predicates instead of raw numeric comparisons.
// ---------------------------------------------------------------------------
#[shared_runtime_test]
async fn sim_predicate_hub_spoke_invariants() {
    let db = isolated_db().await;
    let mut g = GraphBuilder::new(db.pool().clone());

    let anchor = g.add_node("anchor", Team::Blue).await;
    let bridge = g.add_node("bridge", Team::Blue).await;
    g.endorse(anchor, bridge, 1.0).await;

    let hub = g.add_node("red_hub", Team::Red).await;
    g.endorse(bridge, hub, 0.3).await;
    for i in 0..5 {
        let spoke = g.add_node(&format!("red_spoke_{i}"), Team::Red).await;
        g.endorse(hub, spoke, 1.0).await;
    }

    let report = SimulationReport::run(&g, anchor).await;

    let result = predicates::single_attachment_implies_low_diversity(g.spec(), &report);
    assert!(result.holds, "{}: {}", result.name, result.explanation);

    let result = predicates::red_nodes_blocked(g.spec(), &report, 3.0, 2);
    assert!(result.holds, "{}: {}", result.name, result.explanation);
}

// ---------------------------------------------------------------------------
// Predicate test 2: colluding ring invariants
// ---------------------------------------------------------------------------
#[shared_runtime_test]
async fn sim_predicate_ring_invariants() {
    let db = isolated_db().await;
    let mut g = GraphBuilder::new(db.pool().clone());

    let anchor = g.add_node("anchor", Team::Blue).await;
    let bridge = g.add_node("bridge", Team::Blue).await;
    g.endorse(anchor, bridge, 1.0).await;

    let ring = topology::colluding_ring(&mut g, "red", Team::Red, 6, 1.0).await;
    g.endorse(bridge, ring[0], 0.3).await;

    let report = SimulationReport::run(&g, anchor).await;

    let result = predicates::ring_diversity_bounded(g.spec(), &report, &ring, 1);
    assert!(result.holds, "{}: {}", result.name, result.explanation);

    let result = predicates::red_nodes_blocked(g.spec(), &report, 10.0, 2);
    assert!(result.holds, "{}: {}", result.name, result.explanation);
}

// ---------------------------------------------------------------------------
// Predicate test 3: healthy blue network with red attachment
// ---------------------------------------------------------------------------
#[shared_runtime_test]
async fn sim_predicate_healthy_blue_network() {
    let db = isolated_db().await;
    let mut g = GraphBuilder::new(db.pool().clone());

    let anchor = g.add_node("anchor", Team::Blue).await;
    let blue_web = topology::healthy_web(&mut g, "blue", Team::Blue, 6, 0.5, 1.0).await;
    g.endorse(anchor, blue_web[0], 1.0).await;
    g.endorse(anchor, blue_web[1], 1.0).await;

    let (red_hub, _red_spokes) = topology::hub_and_spoke(&mut g, "red", Team::Red, 4, 1.0).await;
    g.endorse(blue_web[2], red_hub, 0.3).await;

    let report = SimulationReport::run(&g, anchor).await;

    let result = predicates::blue_nodes_reachable(g.spec(), &report);
    assert!(result.holds, "{}: {}", result.name, result.explanation);

    let result = predicates::red_nodes_blocked(g.spec(), &report, 10.0, 2);
    assert!(result.holds, "{}: {}", result.name, result.explanation);
}

// ---------------------------------------------------------------------------
// Predicate test 4: denouncer-only revocation mechanism
//
// ADR-024 baseline: when you denounce someone, your endorsement edge to
// them is revoked. Target loses the path; blue nodes are unaffected.
// ---------------------------------------------------------------------------
#[shared_runtime_test]
async fn sim_predicate_denouncer_only_revocation() {
    let db = isolated_db().await;
    let mut g = GraphBuilder::new(db.pool().clone());

    let anchor = g.add_node("anchor", Team::Blue).await;
    let bridge = g.add_node("bridge", Team::Blue).await;
    let target = g.add_node("target", Team::Red).await;

    g.endorse(anchor, bridge, 1.0).await;
    g.endorse(bridge, target, 0.3).await;

    let before_report = SimulationReport::run(&g, anchor).await;
    assert!(
        before_report.distance(target).is_some(),
        "target should be reachable before revocation"
    );

    // Denouncement: bridge revokes its endorsement of target
    g.revoke(bridge, target).await;

    let after_report = SimulationReport::run(&g, anchor).await;

    let result = predicates::red_nodes_blocked(g.spec(), &after_report, 10.0, 1);
    assert!(result.holds, "{}: {}", result.name, result.explanation);

    let result = predicates::blue_nodes_reachable(g.spec(), &after_report);
    assert!(result.holds, "{}: {}", result.name, result.explanation);
}

// ---------------------------------------------------------------------------
// Temporal Scenario 1: Decay reduces edge weight based on age
//
// Three edges with different ages: 1 year old, 6 months old, 1 week old.
// Apply exponential decay (half-life = 6 months = 180 days).
// Assert that older edges have lower weight than newer ones.
// This is a pure GraphSpec test — no database needed.
// ---------------------------------------------------------------------------
#[test]
fn sim_temporal_decay_reduces_weight() {
    use chrono::{Duration, Utc};

    let mut spec = GraphSpec::new();
    let a = Uuid::from_u128(1);
    let b = Uuid::from_u128(2);
    let c = Uuid::from_u128(3);
    let d = Uuid::from_u128(4);
    spec.add_node("a", Team::Blue, a);
    spec.add_node("b", Team::Blue, b);
    spec.add_node("c", Team::Blue, c);
    spec.add_node("d", Team::Blue, d);

    let now = Utc::now();
    let one_year_ago = now - Duration::days(365);
    let six_months_ago = now - Duration::days(180);
    let one_week_ago = now - Duration::days(7);

    spec.add_edge_at(a, b, 1.0, one_year_ago); // oldest
    spec.add_edge_at(a, c, 1.0, six_months_ago); // medium
    spec.add_edge_at(a, d, 1.0, one_week_ago); // newest

    // Exponential decay: weight * 0.5^(age_days / 180)
    let decayed = spec.with_decay(
        now,
        |age| {
            let days = age.num_days() as f32;
            0.5f32.powf(days / 180.0)
        },
        0.01,
    );

    let edges = decayed.all_edges();
    let old_weight = edges[0].weight; // 1 year old
    let mid_weight = edges[1].weight; // 6 months old (half-life)
    let new_weight = edges[2].weight; // 1 week old

    // 1-year-old edge decays to ~0.25 (two half-lives)
    assert!(
        old_weight < 0.3,
        "1-year-old edge should decay below 0.3, got {old_weight:.4}"
    );
    // 6-month-old edge decays to ~0.5 (one half-life)
    assert!(
        (mid_weight - 0.5).abs() < 0.05,
        "6-month-old edge should be near 0.5 (one half-life), got {mid_weight:.4}"
    );
    // 1-week-old edge decays minimally (< 4% loss)
    assert!(
        new_weight > 0.95,
        "1-week-old edge should stay above 0.95, got {new_weight:.4}"
    );
    // Monotonic: older edges have strictly lower weight
    assert!(
        old_weight < mid_weight,
        "1-year edge ({old_weight:.4}) should be less than 6-month edge ({mid_weight:.4})"
    );
    assert!(
        mid_weight < new_weight,
        "6-month edge ({mid_weight:.4}) should be less than 1-week edge ({new_weight:.4})"
    );

    // Original spec is not modified (with_decay returns a new spec)
    let orig_edges = spec.all_edges();
    assert!(
        (orig_edges[0].weight - 1.0).abs() < 0.001,
        "Original spec should be unmodified"
    );
}

// ---------------------------------------------------------------------------
// Temporal Scenario 2: Sybil cluster with old edges decays to near-zero
//
// A Sybil hub-and-spoke has edges created 1 year ago.
// A legitimate node has an edge created 1 week ago.
// After decay, Sybil edges drop near the minimum; legitimate edge stays high.
// This is a pure GraphSpec test — no database needed.
// ---------------------------------------------------------------------------
#[test]
fn sim_temporal_sybil_window_narrows() {
    use chrono::{Duration, Utc};

    let mut spec = GraphSpec::new();

    // UUIDs for the Sybil cluster
    let sybil_hub = Uuid::from_u128(10);
    let sybil_spoke_0 = Uuid::from_u128(11);
    let sybil_spoke_1 = Uuid::from_u128(12);
    let sybil_spoke_2 = Uuid::from_u128(13);

    // UUIDs for the legitimate network
    let legit_anchor = Uuid::from_u128(20);
    let legit_user = Uuid::from_u128(21);

    spec.add_node("sybil_hub", Team::Red, sybil_hub);
    spec.add_node("sybil_spoke_0", Team::Red, sybil_spoke_0);
    spec.add_node("sybil_spoke_1", Team::Red, sybil_spoke_1);
    spec.add_node("sybil_spoke_2", Team::Red, sybil_spoke_2);
    spec.add_node("legit_anchor", Team::Blue, legit_anchor);
    spec.add_node("legit_user", Team::Blue, legit_user);

    let now = Utc::now();
    let one_year_ago = now - Duration::days(365);
    let one_week_ago = now - Duration::days(7);

    // Sybil edges: old (1 year ago)
    spec.add_edge_at(sybil_hub, sybil_spoke_0, 1.0, one_year_ago);
    spec.add_edge_at(sybil_hub, sybil_spoke_1, 1.0, one_year_ago);
    spec.add_edge_at(sybil_hub, sybil_spoke_2, 1.0, one_year_ago);

    // Legitimate edge: recent (1 week ago)
    spec.add_edge_at(legit_anchor, legit_user, 1.0, one_week_ago);

    // Exponential decay with half-life = 6 months; min_weight = 0.01
    let decayed = spec.with_decay(
        now,
        |age| {
            let days = age.num_days() as f32;
            0.5f32.powf(days / 180.0)
        },
        0.01,
    );

    let edges = decayed.all_edges();

    // Sybil edges (old) should decay to near-zero
    for edge in &edges[0..3] {
        assert!(
            edge.weight < 0.3,
            "Sybil edge (1 year old) should decay below 0.3, got {:.4}",
            edge.weight
        );
    }

    // Legitimate edge (recent) should remain high
    let legit_weight = edges[3].weight;
    assert!(
        legit_weight > 0.95,
        "Legitimate 1-week-old edge should stay above 0.95, got {legit_weight:.4}"
    );

    // Confirm the decay gap is substantial (at least 3x difference)
    let sybil_weight = edges[0].weight;
    assert!(
        legit_weight > sybil_weight * 3.0,
        "Legitimate edge ({legit_weight:.4}) should be 3x stronger than Sybil edge ({sybil_weight:.4})"
    );
}
