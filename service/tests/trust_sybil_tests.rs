//! Integration tests for sybil detection and revocation in TrustEngine.
//!
//! Covers hub-and-spoke graph patterns and revocation propagation.

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
        "INSERT INTO reputation__endorsements (endorser_id, subject_id, topic, weight) \
         VALUES ($1, $2, 'trust', $3)",
    )
    .bind(endorser)
    .bind(subject)
    .bind(weight)
    .execute(pool)
    .await
    .unwrap();
}

/// Revoke an existing endorsement by setting revoked_at = now().
async fn revoke_endorsement(pool: &sqlx::PgPool, endorser: Uuid, subject: Uuid) {
    let result = sqlx::query(
        "UPDATE reputation__endorsements SET revoked_at = now() \
         WHERE endorser_id = $1 AND subject_id = $2 AND topic = 'trust' AND revoked_at IS NULL",
    )
    .bind(endorser)
    .bind(subject)
    .execute(pool)
    .await
    .unwrap();
    assert_eq!(
        result.rows_affected(),
        1,
        "expected to revoke exactly 1 endorsement"
    );
}

// ---------------------------------------------------------------------------
// Test 1: Hub-and-spoke gives low diversity
// Graph: H endorses Sybil1, Sybil2, Sybil3. Anchor = H.
// Assert: all three sybils have path_diversity == 1
// ---------------------------------------------------------------------------
#[shared_runtime_test]
async fn test_hub_and_spoke_gives_low_diversity() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let hub = AccountFactory::new()
        .with_seed(1)
        .create(&pool)
        .await
        .expect("create hub");
    let sybil1 = AccountFactory::new()
        .with_seed(2)
        .create(&pool)
        .await
        .expect("create sybil1");
    let sybil2 = AccountFactory::new()
        .with_seed(3)
        .create(&pool)
        .await
        .expect("create sybil2");
    let sybil3 = AccountFactory::new()
        .with_seed(4)
        .create(&pool)
        .await
        .expect("create sybil3");

    insert_endorsement(&pool, hub.id, sybil1.id, 1.0).await;
    insert_endorsement(&pool, hub.id, sybil2.id, 1.0).await;
    insert_endorsement(&pool, hub.id, sybil3.id, 1.0).await;

    let engine = TrustEngine::new(pool);
    let diversities = engine
        .compute_diversity_from(hub.id)
        .await
        .expect("compute_diversity_from");

    for (label, sybil_id) in [
        ("sybil1", sybil1.id),
        ("sybil2", sybil2.id),
        ("sybil3", sybil3.id),
    ] {
        let (_, diversity) = diversities
            .iter()
            .find(|(uid, _)| *uid == sybil_id)
            .unwrap_or_else(|| panic!("{label} should have a diversity entry"));

        assert_eq!(
            *diversity, 1,
            "Hub-and-spoke: {label} endorsed only by hub, expected diversity=1, got {diversity}"
        );
    }
}

// ---------------------------------------------------------------------------
// Test 2: Well-connected user gets high diversity
// Graph: A→B, A→C, A→X, B→X, C→X. Anchor = A.
// Assert: X has diversity=3; B and C have diversity=1
// ---------------------------------------------------------------------------
#[shared_runtime_test]
async fn test_well_connected_user_gets_high_diversity() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let a = AccountFactory::new()
        .with_seed(1)
        .create(&pool)
        .await
        .expect("create a");
    let b = AccountFactory::new()
        .with_seed(2)
        .create(&pool)
        .await
        .expect("create b");
    let c = AccountFactory::new()
        .with_seed(3)
        .create(&pool)
        .await
        .expect("create c");
    let x = AccountFactory::new()
        .with_seed(4)
        .create(&pool)
        .await
        .expect("create x");

    insert_endorsement(&pool, a.id, b.id, 1.0).await;
    insert_endorsement(&pool, a.id, c.id, 1.0).await;
    insert_endorsement(&pool, a.id, x.id, 1.0).await;
    insert_endorsement(&pool, b.id, x.id, 1.0).await;
    insert_endorsement(&pool, c.id, x.id, 1.0).await;

    let engine = TrustEngine::new(pool);
    let diversities = engine
        .compute_diversity_from(a.id)
        .await
        .expect("compute_diversity_from");

    let (_, x_diversity) = diversities
        .iter()
        .find(|(uid, _)| *uid == x.id)
        .expect("X should have a diversity entry");

    assert_eq!(
        *x_diversity, 3,
        "X endorsed by A, B, C (all reachable from A), expected diversity=3, got {x_diversity}"
    );

    let (_, b_diversity) = diversities
        .iter()
        .find(|(uid, _)| *uid == b.id)
        .expect("B should have a diversity entry");

    assert_eq!(
        *b_diversity, 1,
        "B endorsed only by A, expected diversity=1, got {b_diversity}"
    );

    let (_, c_diversity) = diversities
        .iter()
        .find(|(uid, _)| *uid == c.id)
        .expect("C should have a diversity entry");

    assert_eq!(
        *c_diversity, 1,
        "C endorsed only by A, expected diversity=1, got {c_diversity}"
    );
}

// ---------------------------------------------------------------------------
// Test 3: Revocation removes user from graph
// Graph: Anchor → Alice → Bob. Revoke Anchor→Alice.
// Assert: neither Alice nor Bob appear in compute_distances_from results.
// ---------------------------------------------------------------------------
#[shared_runtime_test]
async fn test_revocation_removes_user_from_graph() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let anchor = AccountFactory::new()
        .with_seed(1)
        .create(&pool)
        .await
        .expect("create anchor");
    let alice = AccountFactory::new()
        .with_seed(2)
        .create(&pool)
        .await
        .expect("create alice");
    let bob = AccountFactory::new()
        .with_seed(3)
        .create(&pool)
        .await
        .expect("create bob");

    insert_endorsement(&pool, anchor.id, alice.id, 1.0).await;
    insert_endorsement(&pool, alice.id, bob.id, 1.0).await;

    // Verify both are reachable before revocation
    let engine = TrustEngine::new(pool.clone());
    let scores_before = engine
        .compute_distances_from(anchor.id)
        .await
        .expect("compute_distances_from before");

    assert!(
        scores_before.iter().any(|s| s.user_id == alice.id),
        "Alice should be reachable before revocation"
    );
    assert!(
        scores_before.iter().any(|s| s.user_id == bob.id),
        "Bob should be reachable before revocation"
    );

    // Revoke the Anchor→Alice edge
    revoke_endorsement(&pool, anchor.id, alice.id).await;

    let scores_after = engine
        .compute_distances_from(anchor.id)
        .await
        .expect("compute_distances_from after");

    assert!(
        scores_after.iter().all(|s| s.user_id != alice.id),
        "Alice should not be reachable after Anchor→Alice is revoked"
    );
    assert!(
        scores_after.iter().all(|s| s.user_id != bob.id),
        "Bob should not be reachable after Anchor→Alice is revoked"
    );
}

// ---------------------------------------------------------------------------
// Test 4: Revocation updates diversity
// Graph: Anchor→Alice, Anchor→Bob, Alice→Carol, Bob→Carol.
// Carol has diversity=2. Revoke Anchor→Alice. Carol drops to diversity=1.
// ---------------------------------------------------------------------------
#[shared_runtime_test]
async fn test_revocation_updates_diversity() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let anchor = AccountFactory::new()
        .with_seed(1)
        .create(&pool)
        .await
        .expect("create anchor");
    let alice = AccountFactory::new()
        .with_seed(2)
        .create(&pool)
        .await
        .expect("create alice");
    let bob = AccountFactory::new()
        .with_seed(3)
        .create(&pool)
        .await
        .expect("create bob");
    let carol = AccountFactory::new()
        .with_seed(4)
        .create(&pool)
        .await
        .expect("create carol");

    insert_endorsement(&pool, anchor.id, alice.id, 1.0).await;
    insert_endorsement(&pool, anchor.id, bob.id, 1.0).await;
    insert_endorsement(&pool, alice.id, carol.id, 1.0).await;
    insert_endorsement(&pool, bob.id, carol.id, 1.0).await;

    let engine = TrustEngine::new(pool.clone());

    // Verify Carol has diversity=2 before revocation
    let divs_before = engine
        .compute_diversity_from(anchor.id)
        .await
        .expect("compute_diversity_from before");

    let (_, carol_diversity_before) = divs_before
        .iter()
        .find(|(uid, _)| *uid == carol.id)
        .expect("Carol should have a diversity entry before revocation");

    assert_eq!(
        *carol_diversity_before, 2,
        "Carol endorsed by Alice and Bob, expected diversity=2, got {carol_diversity_before}"
    );

    // Revoke Anchor→Alice
    revoke_endorsement(&pool, anchor.id, alice.id).await;

    let divs_after = engine
        .compute_diversity_from(anchor.id)
        .await
        .expect("compute_diversity_from after");

    let (_, carol_diversity_after) = divs_after
        .iter()
        .find(|(uid, _)| *uid == carol.id)
        .expect("Carol should still have a diversity entry (Bob path still active)");

    assert_eq!(
        *carol_diversity_after, 1,
        "After revoking Anchor→Alice, Carol's diversity should drop to 1, got {carol_diversity_after}"
    );
}

// ---------------------------------------------------------------------------
// Test 5: recompute_from_anchor writes scores
// Graph: Anchor → Alice → Bob.
// Assert: both Alice and Bob have score snapshots with trust_distance set.
// ---------------------------------------------------------------------------
#[shared_runtime_test]
async fn test_recompute_from_anchor_writes_scores() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let anchor = AccountFactory::new()
        .with_seed(1)
        .create(&pool)
        .await
        .expect("create anchor");
    let alice = AccountFactory::new()
        .with_seed(2)
        .create(&pool)
        .await
        .expect("create alice");
    let bob = AccountFactory::new()
        .with_seed(3)
        .create(&pool)
        .await
        .expect("create bob");

    insert_endorsement(&pool, anchor.id, alice.id, 1.0).await;
    insert_endorsement(&pool, alice.id, bob.id, 1.0).await;

    let engine = TrustEngine::new(pool.clone());
    let repo = PgTrustRepo::new(pool.clone());
    engine
        .recompute_from_anchor(anchor.id, &repo)
        .await
        .expect("recompute_from_anchor");

    let alice_score = repo
        .get_score(alice.id, Some(anchor.id))
        .await
        .expect("get_score alice")
        .expect("Alice should have a score snapshot");

    assert!(
        alice_score.trust_distance.is_some(),
        "Alice's score should have trust_distance set"
    );

    let bob_score = repo
        .get_score(bob.id, Some(anchor.id))
        .await
        .expect("get_score bob")
        .expect("Bob should have a score snapshot");

    assert!(
        bob_score.trust_distance.is_some(),
        "Bob's score should have trust_distance set"
    );
}

// ---------------------------------------------------------------------------
// Test 6: Isolated user not reachable
// Graph: Anchor → Alice. Bob has no edges.
// Assert: Bob does not appear in distances or diversity results.
// ---------------------------------------------------------------------------
#[shared_runtime_test]
async fn test_isolated_user_not_reachable() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let anchor = AccountFactory::new()
        .with_seed(1)
        .create(&pool)
        .await
        .expect("create anchor");
    let alice = AccountFactory::new()
        .with_seed(2)
        .create(&pool)
        .await
        .expect("create alice");
    let bob = AccountFactory::new()
        .with_seed(3)
        .create(&pool)
        .await
        .expect("create bob");

    insert_endorsement(&pool, anchor.id, alice.id, 1.0).await;
    // Bob has no edges — completely isolated

    let engine = TrustEngine::new(pool);

    let distances = engine
        .compute_distances_from(anchor.id)
        .await
        .expect("compute_distances_from");

    assert!(
        distances.iter().all(|s| s.user_id != bob.id),
        "Bob (isolated) should not appear in distance results"
    );

    let diversities = engine
        .compute_diversity_from(anchor.id)
        .await
        .expect("compute_diversity_from");

    assert!(
        diversities.iter().all(|(uid, _)| *uid != bob.id),
        "Bob (isolated) should not appear in diversity results"
    );
}
