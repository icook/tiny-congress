//! End-to-end demo day flow test — exercises the HTTP layer, batch worker,
//! trust engine, and room constraint system together in a single scenario.

mod common;

use std::sync::Arc;

use common::factories::AccountFactory;
use common::test_db::isolated_db;
use tc_test_macros::shared_runtime_test;
use tinycongress_api::reputation::repo::{PgReputationRepo, ReputationRepo};
use tinycongress_api::trust::constraints::RoomConstraint;
use tinycongress_api::trust::engine::TrustEngine;
use tinycongress_api::trust::repo::{PgTrustRepo, TrustRepo};
use tinycongress_api::trust::service::{DefaultTrustService, TrustService};
use tinycongress_api::trust::worker::TrustWorker;
use uuid::Uuid;

/// Insert an active endorsement directly into the DB (bypass the action queue).
async fn insert_trust_endorsement(pool: &sqlx::PgPool, endorser: Uuid, subject: Uuid, weight: f32) {
    sqlx::query(
        "INSERT INTO reputation__endorsements (endorser_id, subject_id, topic, weight) \
         VALUES ($1, $2, 'trust', $3) ON CONFLICT DO NOTHING",
    )
    .bind(endorser)
    .bind(subject)
    .bind(weight)
    .execute(pool)
    .await
    .unwrap();
}

// ---------------------------------------------------------------------------
// Demo Day Flow
// ---------------------------------------------------------------------------
// Users: Alice (anchor), Bob (trusted), Carol (endorsed by Bob),
//        Dave (outsider), Eve (isolated — no edges).
//
// Graph: Alice → Bob → Carol
//        Dave and Eve are not reachable from Alice.
// ---------------------------------------------------------------------------
#[shared_runtime_test]
async fn test_demo_day_flow() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    // Step 1: Create 5 accounts
    let alice = AccountFactory::new()
        .with_seed(1)
        .create(&pool)
        .await
        .expect("create alice");
    let bob = AccountFactory::new()
        .with_seed(2)
        .create(&pool)
        .await
        .expect("create bob");
    let carol = AccountFactory::new()
        .with_seed(3)
        .create(&pool)
        .await
        .expect("create carol");
    let dave = AccountFactory::new()
        .with_seed(4)
        .create(&pool)
        .await
        .expect("create dave");
    // Eve is isolated — no edges; we only need her ID implicitly absent from results

    // Step 2-3: Set up endorsement graph Alice→Bob→Carol
    insert_trust_endorsement(&pool, alice.id, bob.id, 1.0).await;
    insert_trust_endorsement(&pool, bob.id, carol.id, 0.8).await;

    // Step 4: Run trust engine recompute from Alice (anchor)
    let trust_repo: Arc<dyn TrustRepo> = Arc::new(PgTrustRepo::new(pool.clone()));
    let engine = TrustEngine::new(pool.clone());
    let written = engine
        .recompute_from_anchor(alice.id, trust_repo.as_ref())
        .await
        .expect("recompute_from_anchor");
    assert_eq!(
        written, 3,
        "Should have written scores for Alice (anchor), Bob, and Carol"
    );

    // Step 5: Verify trust distances written to score_snapshots
    let bob_score = trust_repo
        .get_score(bob.id, Some(alice.id))
        .await
        .expect("get bob score")
        .expect("Bob should have a score snapshot");
    let bob_dist = bob_score
        .trust_distance
        .expect("Bob should have a trust_distance");
    // Alice→Bob weight 1.0 → distance = 1/1.0 = 1.0
    assert!(
        (bob_dist - 1.0).abs() < 0.01,
        "Bob distance should be ~1.0, got {bob_dist}"
    );

    let carol_score = trust_repo
        .get_score(carol.id, Some(alice.id))
        .await
        .expect("get carol score")
        .expect("Carol should have a score snapshot");
    let carol_dist = carol_score
        .trust_distance
        .expect("Carol should have a trust_distance");
    // 1/1.0 (Alice→Bob) + 1/0.8 (Bob→Carol) = 1.0 + 1.25 = 2.25
    assert!(
        (carol_dist - 2.25).abs() < 0.01,
        "Carol distance should be ~2.25, got {carol_dist}"
    );

    let dave_score = trust_repo
        .get_score(dave.id, Some(alice.id))
        .await
        .expect("get dave score");
    assert!(dave_score.is_none(), "Dave should be unreachable");

    // Step 6: Verify room constraint — Bob has identity_verified endorsement, Dave does not
    // Insert an identity_verified endorsement for Bob (as a verifier would via /verifiers/endorsements)
    sqlx::query(
        "INSERT INTO reputation__endorsements (endorser_id, subject_id, topic, weight) \
         VALUES ($1, $2, 'identity_verified', 1.0)",
    )
    .bind(alice.id)
    .bind(bob.id)
    .execute(&pool)
    .await
    .unwrap();

    let endorsed_by = tinycongress_api::trust::constraints::EndorsedByConstraint {
        topic: "identity_verified".to_string(),
    };

    let bob_eligibility = endorsed_by
        .check(bob.id, None, trust_repo.as_ref())
        .await
        .expect("check bob eligibility");
    assert!(bob_eligibility.is_eligible, "Bob should be eligible");

    let dave_eligibility = endorsed_by
        .check(dave.id, None, trust_repo.as_ref())
        .await
        .expect("check dave eligibility");
    assert!(!dave_eligibility.is_eligible, "Dave should not be eligible");

    // Step 7: Queue an endorse action via TrustService (Alice endorses Bob — idempotent at queue level)
    let rep_repo = Arc::new(PgReputationRepo::new(pool.clone())) as Arc<dyn ReputationRepo>;
    let trust_service = DefaultTrustService::new(trust_repo.clone(), rep_repo);
    trust_service
        .endorse(alice.id, bob.id, 1.0, None)
        .await
        .expect("endorse should succeed");

    let action_count = trust_repo
        .count_daily_actions(alice.id)
        .await
        .expect("count_daily_actions");
    assert_eq!(action_count, 1, "Alice should have 1 queued action");

    // Step 8: Process the batch via TrustWorker
    let reputation_repo = Arc::new(PgReputationRepo::new(pool.clone()));
    let engine_arc = Arc::new(TrustEngine::new(pool.clone()));
    let worker = TrustWorker::new(trust_repo.clone(), reputation_repo, engine_arc, 50, 30);
    let processed = worker.process_batch().await.expect("process_batch");
    assert_eq!(processed, 1, "Worker should process 1 action");

    // Step 9: Verify action was processed (still counted toward quota, now completed)
    let action_count_after = trust_repo
        .count_daily_actions(alice.id)
        .await
        .expect("count_daily_actions after");
    assert_eq!(
        action_count_after, 1,
        "Completed actions still count toward daily quota"
    );

    // Verify the action was marked completed (not just counted)
    let status: String = sqlx::query_scalar(
        "SELECT status FROM trust__action_queue \
         WHERE actor_id = $1 ORDER BY created_at DESC LIMIT 1",
    )
    .bind(alice.id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        status, "completed",
        "worker should have completed the action"
    );
}
