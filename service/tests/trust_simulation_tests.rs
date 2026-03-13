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
    // Observed: distance=5.333 for all spokes (matches expected math)
    for &spoke in &spokes {
        let score = distances
            .iter()
            .find(|s| s.user_id == spoke)
            .expect("spoke should be reachable");
        let dist = score.trust_distance.expect("spoke should have distance");
        assert!(
            dist >= 3.0,
            "Hub-and-spoke: spoke distance should be >= 3.0 (Congress threshold), got {dist:.3}"
        );
    }
}
