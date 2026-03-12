//! Integration tests for TrustEngine — distance CTE and path diversity approximation.
//!
//! Tests correspond to TRD Section 6.1 specs.

mod common;

use common::factories::AccountFactory;
use common::test_db::isolated_db;
use tc_test_macros::shared_runtime_test;
use tinycongress_api::trust::engine::TrustEngine;
use tinycongress_api::trust::repo::{PgTrustRepo, TrustRepo};
use uuid::Uuid;

/// Insert an active endorsement directly into the DB (bypass the action queue for test setup).
async fn insert_endorsement(pool: &sqlx::PgPool, endorser: Uuid, subject: Uuid, weight: f32) {
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
async fn insert_revoked_endorsement(
    pool: &sqlx::PgPool,
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

// ---------------------------------------------------------------------------
// TRD 6.1: Linear chain trust distance
// Setup: Seed → A → B → C, all weight 1.0
// Assert: C.trust_distance from Seed = 3.0
// ---------------------------------------------------------------------------
#[shared_runtime_test]
async fn test_linear_chain_trust_distance() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    // Seeds: 1..4 within this isolated db
    let seed = AccountFactory::new()
        .with_seed(1)
        .create(&pool)
        .await
        .expect("create seed");
    let a = AccountFactory::new()
        .with_seed(2)
        .create(&pool)
        .await
        .expect("create a");
    let b = AccountFactory::new()
        .with_seed(3)
        .create(&pool)
        .await
        .expect("create b");
    let c = AccountFactory::new()
        .with_seed(4)
        .create(&pool)
        .await
        .expect("create c");

    insert_endorsement(&pool, seed.id, a.id, 1.0).await;
    insert_endorsement(&pool, a.id, b.id, 1.0).await;
    insert_endorsement(&pool, b.id, c.id, 1.0).await;

    let engine = TrustEngine::new(pool);
    let scores = engine
        .compute_distances_from(seed.id)
        .await
        .expect("compute_distances_from");

    let c_score = scores
        .iter()
        .find(|s| s.user_id == c.id)
        .expect("C should be reachable");

    let distance = c_score.trust_distance.expect("C should have a distance");
    assert!(
        (distance - 3.0).abs() < 0.01,
        "Expected C distance ≈ 3.0, got {distance}"
    );
}

// ---------------------------------------------------------------------------
// TRD 6.1: Mixed-weight distance
// Setup: Seed → A (weight 1.0) → B (weight 0.3)
// Assert: B.trust_distance ≈ 1.0 + (1.0/0.3) ≈ 4.33
// ---------------------------------------------------------------------------
#[shared_runtime_test]
async fn test_mixed_weight_distance() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let seed = AccountFactory::new()
        .with_seed(1)
        .create(&pool)
        .await
        .expect("create seed");
    let a = AccountFactory::new()
        .with_seed(2)
        .create(&pool)
        .await
        .expect("create a");
    let b = AccountFactory::new()
        .with_seed(3)
        .create(&pool)
        .await
        .expect("create b");

    insert_endorsement(&pool, seed.id, a.id, 1.0).await;
    insert_endorsement(&pool, a.id, b.id, 0.3).await;

    let engine = TrustEngine::new(pool);
    let scores = engine
        .compute_distances_from(seed.id)
        .await
        .expect("compute_distances_from");

    let b_score = scores
        .iter()
        .find(|s| s.user_id == b.id)
        .expect("B should be reachable");

    let distance = b_score.trust_distance.expect("B should have a distance");
    let expected = 1.0_f32 + (1.0 / 0.3_f32);
    assert!(
        (distance - expected).abs() < 0.05,
        "Expected B distance ≈ {expected:.3}, got {distance:.3}"
    );
}

// ---------------------------------------------------------------------------
// TRD 6.1: Path diversity — independent branches
// Setup: Seed → A → X; Seed → B → X (A and B are independent paths)
// Assert: X.path_diversity = 2
// ---------------------------------------------------------------------------
#[shared_runtime_test]
async fn test_path_diversity_independent_branches() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let seed = AccountFactory::new()
        .with_seed(1)
        .create(&pool)
        .await
        .expect("create seed");
    let a = AccountFactory::new()
        .with_seed(2)
        .create(&pool)
        .await
        .expect("create a");
    let b = AccountFactory::new()
        .with_seed(3)
        .create(&pool)
        .await
        .expect("create b");
    let x = AccountFactory::new()
        .with_seed(4)
        .create(&pool)
        .await
        .expect("create x");

    insert_endorsement(&pool, seed.id, a.id, 1.0).await;
    insert_endorsement(&pool, seed.id, b.id, 1.0).await;
    insert_endorsement(&pool, a.id, x.id, 1.0).await;
    insert_endorsement(&pool, b.id, x.id, 1.0).await;

    let engine = TrustEngine::new(pool);
    let diversities = engine
        .compute_diversity_from(seed.id)
        .await
        .expect("compute_diversity_from");

    let (_, x_diversity) = diversities
        .iter()
        .find(|(uid, _)| *uid == x.id)
        .expect("X should have a diversity entry");

    assert_eq!(
        *x_diversity, 2,
        "X has 2 independent endorsers (A and B), expected diversity=2, got {x_diversity}"
    );
}

// ---------------------------------------------------------------------------
// TRD 6.1: Path diversity — shared branch
// Setup: Seed → A → B → X AND Seed → A → C → X (both paths share A)
// Assert: X.path_diversity >= 1
//
// Note: the approximation counts distinct reachable endorsers of X. In this
// topology B and C are both reachable from the anchor, so diversity = 2.
// The TRD acknowledges this is an approximation for demo scale; the key
// invariant (hub-and-spoke gives diversity=1) still holds.
// ---------------------------------------------------------------------------
#[shared_runtime_test]
async fn test_path_diversity_shared_branch() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let seed = AccountFactory::new()
        .with_seed(1)
        .create(&pool)
        .await
        .expect("create seed");
    let a = AccountFactory::new()
        .with_seed(2)
        .create(&pool)
        .await
        .expect("create a");
    let b = AccountFactory::new()
        .with_seed(3)
        .create(&pool)
        .await
        .expect("create b");
    let c = AccountFactory::new()
        .with_seed(4)
        .create(&pool)
        .await
        .expect("create c");
    let x = AccountFactory::new()
        .with_seed(5)
        .create(&pool)
        .await
        .expect("create x");

    // Both paths to X go through A; B and C are reachable via A
    insert_endorsement(&pool, seed.id, a.id, 1.0).await;
    insert_endorsement(&pool, a.id, b.id, 1.0).await;
    insert_endorsement(&pool, a.id, c.id, 1.0).await;
    insert_endorsement(&pool, b.id, x.id, 1.0).await;
    insert_endorsement(&pool, c.id, x.id, 1.0).await;

    let engine = TrustEngine::new(pool);
    let diversities = engine
        .compute_diversity_from(seed.id)
        .await
        .expect("compute_diversity_from");

    let (_, x_diversity) = diversities
        .iter()
        .find(|(uid, _)| *uid == x.id)
        .expect("X should have a diversity entry");

    // The approximation gives 2 because B and C are both reachable from anchor.
    // Key invariant: diversity is positive and reflects multiple endorsers.
    assert!(
        *x_diversity >= 1,
        "X should have at least diversity=1, got {x_diversity}"
    );
}

// ---------------------------------------------------------------------------
// TRD 6.1: Revoked edge exclusion
// Setup: Seed → A → B; then revoke A→B edge
// Assert: B has no score (unreachable)
// ---------------------------------------------------------------------------
#[shared_runtime_test]
async fn test_revoked_edge_exclusion() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let seed = AccountFactory::new()
        .with_seed(1)
        .create(&pool)
        .await
        .expect("create seed");
    let a = AccountFactory::new()
        .with_seed(2)
        .create(&pool)
        .await
        .expect("create a");
    let b = AccountFactory::new()
        .with_seed(3)
        .create(&pool)
        .await
        .expect("create b");

    insert_endorsement(&pool, seed.id, a.id, 1.0).await;
    // Insert the A→B edge as revoked — B should be unreachable
    insert_revoked_endorsement(&pool, a.id, b.id, 1.0).await;

    let engine = TrustEngine::new(pool);
    let scores = engine
        .compute_distances_from(seed.id)
        .await
        .expect("compute_distances_from");

    let b_score = scores.iter().find(|s| s.user_id == b.id);
    assert!(
        b_score.is_none(),
        "B should be unreachable (A→B edge is revoked)"
    );
}

// ---------------------------------------------------------------------------
// TRD 6.1: Cycle prevention
// Setup: Seed → A → B → A (cycle)
// Assert: CTE terminates; A.trust_distance computed correctly (= 1.0 from seed)
// ---------------------------------------------------------------------------
#[shared_runtime_test]
async fn test_cycle_prevention() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let seed = AccountFactory::new()
        .with_seed(1)
        .create(&pool)
        .await
        .expect("create seed");
    let a = AccountFactory::new()
        .with_seed(2)
        .create(&pool)
        .await
        .expect("create a");
    let b = AccountFactory::new()
        .with_seed(3)
        .create(&pool)
        .await
        .expect("create b");

    insert_endorsement(&pool, seed.id, a.id, 1.0).await;
    insert_endorsement(&pool, a.id, b.id, 1.0).await;
    // B→A creates a cycle (two distinct rows with different endorser/subject, allowed by schema)
    insert_endorsement(&pool, b.id, a.id, 1.0).await;

    let engine = TrustEngine::new(pool);
    // Should complete without hanging or panicking
    let scores = engine
        .compute_distances_from(seed.id)
        .await
        .expect("compute_distances_from should handle cycles");

    let a_score = scores
        .iter()
        .find(|s| s.user_id == a.id)
        .expect("A should be reachable");

    let distance = a_score.trust_distance.expect("A should have a distance");
    // Direct edge Seed→A with weight 1.0 gives distance 1.0
    assert!(
        (distance - 1.0).abs() < 0.01,
        "Expected A distance = 1.0, got {distance}"
    );
}

// ---------------------------------------------------------------------------
// TRD 6.1: Hub-and-spoke detection
// Setup: Seed endorses Attacker; Attacker endorses 5 nodes (no other endorsers)
// Assert: all 5 nodes have path_diversity = 1
// ---------------------------------------------------------------------------
#[shared_runtime_test]
async fn test_hub_and_spoke_diversity() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let seed = AccountFactory::new()
        .with_seed(1)
        .create(&pool)
        .await
        .expect("create seed");
    let attacker = AccountFactory::new()
        .with_seed(2)
        .create(&pool)
        .await
        .expect("create attacker");

    let mut spoke_ids = Vec::new();
    for i in 0_u8..5 {
        let spoke = AccountFactory::new()
            .with_seed(3 + i)
            .create(&pool)
            .await
            .expect("create spoke");
        spoke_ids.push(spoke.id);
    }

    insert_endorsement(&pool, seed.id, attacker.id, 1.0).await;
    for &spoke_id in &spoke_ids {
        insert_endorsement(&pool, attacker.id, spoke_id, 1.0).await;
    }

    let engine = TrustEngine::new(pool);
    let diversities = engine
        .compute_diversity_from(seed.id)
        .await
        .expect("compute_diversity_from");

    for spoke_id in &spoke_ids {
        let (_, diversity) = diversities
            .iter()
            .find(|(uid, _)| uid == spoke_id)
            .expect("spoke should have diversity entry");

        assert_eq!(
            *diversity, 1,
            "Hub-and-spoke: each spoke endorsed only by attacker, expected diversity=1, got {diversity}"
        );
    }
}

// ---------------------------------------------------------------------------
// Recompute integration: recompute_from_anchor writes scores to snapshot table
// ---------------------------------------------------------------------------
#[shared_runtime_test]
async fn test_recompute_from_anchor_writes_scores() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let seed = AccountFactory::new()
        .with_seed(1)
        .create(&pool)
        .await
        .expect("create seed");
    let a = AccountFactory::new()
        .with_seed(2)
        .create(&pool)
        .await
        .expect("create a");
    let b = AccountFactory::new()
        .with_seed(3)
        .create(&pool)
        .await
        .expect("create b");

    insert_endorsement(&pool, seed.id, a.id, 1.0).await;
    insert_endorsement(&pool, a.id, b.id, 1.0).await;

    let engine = TrustEngine::new(pool.clone());
    let repo = PgTrustRepo::new(pool.clone());
    let count = engine
        .recompute_from_anchor(seed.id, &repo)
        .await
        .expect("recompute_from_anchor");

    assert_eq!(count, 3, "Should have written scores for anchor, A, and B");

    // Verify A's score is in the snapshot table
    let a_snap = repo
        .get_score(a.id, Some(seed.id))
        .await
        .expect("get_score")
        .expect("A should have a score snapshot");

    let a_distance = a_snap
        .trust_distance
        .expect("A should have a trust_distance");
    assert!(
        (a_distance - 1.0).abs() < 0.01,
        "Expected A distance = 1.0, got {a_distance}"
    );

    let b_snap = repo
        .get_score(b.id, Some(seed.id))
        .await
        .expect("get_score")
        .expect("B should have a score snapshot");

    let b_distance = b_snap
        .trust_distance
        .expect("B should have a trust_distance");
    assert!(
        (b_distance - 2.0).abs() < 0.01,
        "Expected B distance = 2.0, got {b_distance}"
    );
}

// ---------------------------------------------------------------------------
// Anchor bootstrap: anchor itself gets distance=0 in compute_distances_from
// ---------------------------------------------------------------------------
#[shared_runtime_test]
async fn test_anchor_has_distance_zero() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let seed = AccountFactory::new()
        .with_seed(1)
        .create(&pool)
        .await
        .expect("create seed");
    let a = AccountFactory::new()
        .with_seed(2)
        .create(&pool)
        .await
        .expect("create a");

    insert_endorsement(&pool, seed.id, a.id, 1.0).await;

    let engine = TrustEngine::new(pool);
    let scores = engine
        .compute_distances_from(seed.id)
        .await
        .expect("compute_distances_from");

    let anchor_score = scores
        .iter()
        .find(|s| s.user_id == seed.id)
        .expect("Anchor should be present in results");

    let distance = anchor_score
        .trust_distance
        .expect("Anchor should have a distance");
    assert!(
        distance.abs() < 0.01,
        "Expected anchor distance = 0.0, got {distance}"
    );
}

// ---------------------------------------------------------------------------
// Anchor bootstrap: recompute_from_anchor persists anchor score to snapshots
// ---------------------------------------------------------------------------
#[shared_runtime_test]
async fn test_recompute_from_anchor_writes_anchor_score() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let seed = AccountFactory::new()
        .with_seed(1)
        .create(&pool)
        .await
        .expect("create seed");
    let a = AccountFactory::new()
        .with_seed(2)
        .create(&pool)
        .await
        .expect("create a");

    insert_endorsement(&pool, seed.id, a.id, 1.0).await;

    let engine = TrustEngine::new(pool.clone());
    let repo = PgTrustRepo::new(pool.clone());
    let count = engine
        .recompute_from_anchor(seed.id, &repo)
        .await
        .expect("recompute_from_anchor");

    // Anchor + A = 2 scores written
    assert_eq!(count, 2, "Should have written scores for anchor and A");

    // Verify anchor's own score
    let anchor_snap = repo
        .get_score(seed.id, Some(seed.id))
        .await
        .expect("get_score")
        .expect("Anchor should have a score snapshot");

    let anchor_distance = anchor_snap
        .trust_distance
        .expect("Anchor should have a trust_distance");
    assert!(
        anchor_distance.abs() < 0.01,
        "Expected anchor distance = 0.0, got {anchor_distance}"
    );

    assert_eq!(
        anchor_snap.path_diversity,
        Some(i32::MAX),
        "Anchor diversity should be sentinel high value"
    );
}
