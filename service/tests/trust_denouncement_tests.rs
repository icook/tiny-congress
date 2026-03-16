//! Integration tests for trust denouncement repository operations.

mod common;

use std::sync::Arc;

use common::factories::{insert_endorsement, AccountFactory};
use common::test_db::isolated_db;
use serde_json::json;
use tc_test_macros::shared_runtime_test;
use tinycongress_api::reputation::repo::{PgReputationRepo, ReputationRepo};
use tinycongress_api::trust::engine::TrustEngine;
use tinycongress_api::trust::repo::{PgTrustRepo, TrustRepo, TrustRepoError};
use tinycongress_api::trust::service::{DefaultTrustService, TrustService, TrustServiceError};
use tinycongress_api::trust::worker::TrustWorker;

#[shared_runtime_test]
async fn test_create_denouncement() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let accuser = AccountFactory::new()
        .with_seed(50)
        .create(&pool)
        .await
        .expect("create accuser");

    let target = AccountFactory::new()
        .with_seed(51)
        .create(&pool)
        .await
        .expect("create target");

    let repo = PgTrustRepo::new(pool);
    let record = repo
        .create_denouncement(accuser.id, target.id, "spam behavior")
        .await
        .expect("create_denouncement");

    assert_eq!(record.accuser_id, accuser.id);
    assert_eq!(record.target_id, target.id);
    assert_eq!(record.reason, "spam behavior");
    assert!(record.resolved_at.is_none());
}

#[shared_runtime_test]
async fn test_duplicate_denouncement_rejected() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let accuser = AccountFactory::new()
        .with_seed(52)
        .create(&pool)
        .await
        .expect("create accuser");

    let target = AccountFactory::new()
        .with_seed(53)
        .create(&pool)
        .await
        .expect("create target");

    let repo = PgTrustRepo::new(pool);
    repo.create_denouncement(accuser.id, target.id, "first")
        .await
        .expect("first denouncement");

    let result = repo
        .create_denouncement(accuser.id, target.id, "second")
        .await;

    assert!(
        matches!(result, Err(TrustRepoError::Duplicate)),
        "expected Duplicate, got: {result:?}"
    );
}

#[shared_runtime_test]
async fn test_list_denouncements_against() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let accuser1 = AccountFactory::new()
        .with_seed(54)
        .create(&pool)
        .await
        .expect("create accuser1");

    let accuser2 = AccountFactory::new()
        .with_seed(55)
        .create(&pool)
        .await
        .expect("create accuser2");

    let target = AccountFactory::new()
        .with_seed(56)
        .create(&pool)
        .await
        .expect("create target");

    let repo = PgTrustRepo::new(pool);
    repo.create_denouncement(accuser1.id, target.id, "reason one")
        .await
        .expect("denouncement 1");
    repo.create_denouncement(accuser2.id, target.id, "reason two")
        .await
        .expect("denouncement 2");

    let list = repo
        .list_denouncements_against(target.id)
        .await
        .expect("list_denouncements_against");

    assert_eq!(list.len(), 2);
    assert!(list.iter().all(|d| d.target_id == target.id));
}

#[shared_runtime_test]
async fn test_count_active_denouncements_by() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let accuser = AccountFactory::new()
        .with_seed(57)
        .create(&pool)
        .await
        .expect("create accuser");

    let target1 = AccountFactory::new()
        .with_seed(58)
        .create(&pool)
        .await
        .expect("create target1");

    let target2 = AccountFactory::new()
        .with_seed(59)
        .create(&pool)
        .await
        .expect("create target2");

    let repo = PgTrustRepo::new(pool);
    repo.create_denouncement(accuser.id, target1.id, "reason")
        .await
        .expect("denouncement 1");
    repo.create_denouncement(accuser.id, target2.id, "reason")
        .await
        .expect("denouncement 2");

    let count = repo
        .count_active_denouncements_by(accuser.id)
        .await
        .expect("count_active_denouncements_by");

    assert_eq!(count, 2);
}

/// Resolved denouncements still count toward the permanent budget (non-refundable).
#[shared_runtime_test]
async fn test_resolved_denouncement_still_counts() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let accuser = AccountFactory::new()
        .with_seed(60)
        .create(&pool)
        .await
        .expect("create accuser");

    let target = AccountFactory::new()
        .with_seed(61)
        .create(&pool)
        .await
        .expect("create target");

    let repo = PgTrustRepo::new(pool.clone());
    repo.create_denouncement(accuser.id, target.id, "spam")
        .await
        .expect("denouncement");

    // Simulate admin resolving the denouncement
    sqlx::query(
        "UPDATE trust__denouncements SET resolved_at = NOW() \
         WHERE accuser_id = $1 AND target_id = $2",
    )
    .bind(accuser.id)
    .bind(target.id)
    .execute(&pool)
    .await
    .expect("resolve denouncement");

    // The count must still be 1 — resolving does not refund the slot
    let count = repo
        .count_active_denouncements_by(accuser.id)
        .await
        .expect("count");
    assert_eq!(count, 1);
}

// ---------------------------------------------------------------------------
// Task 2: Edge Revocation on Denouncement
// ---------------------------------------------------------------------------

/// When A denounces B and A has an active endorsement of B, the worker must
/// revoke that endorsement (set revoked_at) as part of processing the action.
#[shared_runtime_test]
async fn denouncement_revokes_endorsement_edge() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let actor = AccountFactory::new()
        .with_seed(80)
        .create(&pool)
        .await
        .expect("create actor");

    let target = AccountFactory::new()
        .with_seed(81)
        .create(&pool)
        .await
        .expect("create target");

    // A endorses B first
    insert_endorsement(&pool, actor.id, target.id, 1.0).await;

    // Verify endorsement exists (active, not revoked)
    let active_before: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM reputation__endorsements \
         WHERE endorser_id = $1 AND subject_id = $2 AND revoked_at IS NULL",
    )
    .bind(actor.id)
    .bind(target.id)
    .fetch_one(&pool)
    .await
    .expect("count before");
    assert_eq!(
        active_before, 1,
        "endorsement should exist before denouncement"
    );

    // Seed a 'denounce' action
    sqlx::query(
        "INSERT INTO trust__action_queue (actor_id, action_type, payload) VALUES ($1, $2, $3)",
    )
    .bind(actor.id)
    .bind("denounce")
    .bind(json!({ "target_id": target.id, "reason": "bad actor" }))
    .execute(&pool)
    .await
    .expect("seed denounce action");

    let trust_repo = Arc::new(PgTrustRepo::new(pool.clone()));
    let reputation_repo = Arc::new(PgReputationRepo::new(pool.clone()));
    let engine = Arc::new(TrustEngine::new(pool.clone()));
    let worker = Arc::new(TrustWorker::new(
        trust_repo,
        reputation_repo,
        engine,
        50,
        30,
    ));

    let processed = worker.process_batch().await.expect("process_batch");
    assert_eq!(processed, 1, "one action should be processed");

    // Denouncement row should exist
    let denouncement_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM trust__denouncements WHERE accuser_id = $1 AND target_id = $2",
    )
    .bind(actor.id)
    .bind(target.id)
    .fetch_one(&pool)
    .await
    .expect("count denouncements");
    assert_eq!(denouncement_count, 1, "denouncement row should exist");

    // Endorsement should now be revoked
    let revoked_at: Option<chrono::DateTime<chrono::Utc>> = sqlx::query_scalar(
        "SELECT revoked_at FROM reputation__endorsements \
         WHERE endorser_id = $1 AND subject_id = $2",
    )
    .bind(actor.id)
    .bind(target.id)
    .fetch_one(&pool)
    .await
    .expect("fetch endorsement");
    assert!(
        revoked_at.is_some(),
        "endorsement should be revoked after denouncement"
    );
}

// ---------------------------------------------------------------------------
// Task 3: Denouncement Without Existing Endorsement
// ---------------------------------------------------------------------------

/// Denouncing without an existing endorsement should succeed — the revocation
/// step is a no-op and should not fail the action.
#[shared_runtime_test]
async fn denouncement_without_endorsement_succeeds() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let actor = AccountFactory::new()
        .with_seed(82)
        .create(&pool)
        .await
        .expect("create actor");

    let target = AccountFactory::new()
        .with_seed(83)
        .create(&pool)
        .await
        .expect("create target");

    // No endorsement exists between actor and target

    // Seed a 'denounce' action
    sqlx::query(
        "INSERT INTO trust__action_queue (actor_id, action_type, payload) VALUES ($1, $2, $3)",
    )
    .bind(actor.id)
    .bind("denounce")
    .bind(json!({ "target_id": target.id, "reason": "suspicious" }))
    .execute(&pool)
    .await
    .expect("seed denounce action");

    let trust_repo = Arc::new(PgTrustRepo::new(pool.clone()));
    let reputation_repo = Arc::new(PgReputationRepo::new(pool.clone()));
    let engine = Arc::new(TrustEngine::new(pool.clone()));
    let worker = Arc::new(TrustWorker::new(
        trust_repo,
        reputation_repo,
        engine,
        50,
        30,
    ));

    let processed = worker.process_batch().await.expect("process_batch");
    assert_eq!(processed, 1, "one action should be processed");

    // Action should be completed (not failed)
    let (status,): (String,) =
        sqlx::query_as("SELECT status FROM trust__action_queue WHERE actor_id = $1")
            .bind(actor.id)
            .fetch_one(&pool)
            .await
            .expect("fetch action status");
    assert_eq!(
        status, "completed",
        "denounce-without-endorsement should complete successfully"
    );

    // Denouncement row should exist
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM trust__denouncements WHERE accuser_id = $1 AND target_id = $2",
    )
    .bind(actor.id)
    .bind(target.id)
    .fetch_one(&pool)
    .await
    .expect("count");
    assert_eq!(count, 1, "denouncement row should exist");
}

// ---------------------------------------------------------------------------
// Task 4: Mutual Exclusion — Cannot Endorse After Denouncing
// ---------------------------------------------------------------------------

/// After A denounces B, A must not be able to endorse B.
/// The service layer must reject the endorsement attempt with DenouncementConflict.
#[shared_runtime_test]
async fn cannot_endorse_someone_you_denounced() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let endorser = AccountFactory::new()
        .with_seed(90)
        .create(&pool)
        .await
        .expect("create endorser");

    let target = AccountFactory::new()
        .with_seed(91)
        .create(&pool)
        .await
        .expect("create target");

    // Insert an active denouncement directly to simulate A having denounced B
    let repo = PgTrustRepo::new(pool.clone());
    repo.create_denouncement(endorser.id, target.id, "bad actor")
        .await
        .expect("create denouncement");

    // Now try to endorse — should be rejected
    let rep_repo = Arc::new(PgReputationRepo::new(pool.clone())) as Arc<dyn ReputationRepo>;
    let trust_repo = Arc::new(PgTrustRepo::new(pool.clone()));
    let service = DefaultTrustService::new(trust_repo, rep_repo);

    let result = service.endorse(endorser.id, target.id, 1.0, None).await;

    assert!(
        matches!(result, Err(TrustServiceError::DenouncementConflict)),
        "expected DenouncementConflict, got: {result:?}"
    );
}
