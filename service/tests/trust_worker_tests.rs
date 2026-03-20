//! Integration tests for TrustWorker pgmq-based processing.

mod common;

use std::sync::Arc;

use common::factories::AccountFactory;
use common::test_db::isolated_db;
use serde_json::json;
use tc_test_macros::shared_runtime_test;
use tinycongress_api::reputation::repo::PgReputationRepo;
use tinycongress_api::trust::engine::TrustEngine;
use tinycongress_api::trust::repo::{PgTrustRepo, TrustRepo};
use tinycongress_api::trust::worker::TrustWorker;

/// Build a worker against the given pool.
fn make_worker(pool: sqlx::PgPool) -> Arc<TrustWorker> {
    let trust_repo = Arc::new(PgTrustRepo::new(pool.clone()));
    let reputation_repo = Arc::new(PgReputationRepo::new(pool.clone()));
    let engine = Arc::new(TrustEngine::new(pool.clone()));
    Arc::new(TrustWorker::new(pool, trust_repo, reputation_repo, engine))
}

// ---------------------------------------------------------------------------
// Test 1: endorse action — creates endorsement and completes the action
// ---------------------------------------------------------------------------
#[shared_runtime_test]
async fn test_process_batch_endorse_action() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let actor = AccountFactory::new()
        .with_seed(1)
        .create(&pool)
        .await
        .expect("create actor");

    let subject = AccountFactory::new()
        .with_seed(2)
        .create(&pool)
        .await
        .expect("create subject");

    // Enqueue via repo (inserts into trust__action_log and sends pgmq message)
    let trust_repo = PgTrustRepo::new(pool.clone());
    trust_repo
        .enqueue_action(
            actor.id,
            "endorse",
            &json!({ "subject_id": subject.id, "weight": 0.8, "attestation": null, "in_slot": true }),
        )
        .await
        .expect("enqueue action");

    let worker = make_worker(pool.clone());
    let processed = worker.process_one().await.expect("process_one");
    assert!(processed, "expected a message to be processed");

    // Action should be marked completed
    let (status,): (String,) =
        sqlx::query_as("SELECT status FROM trust__action_log WHERE actor_id = $1")
            .bind(actor.id)
            .fetch_one(&pool)
            .await
            .expect("fetch action");
    assert_eq!(status, "completed");

    // Endorsement row should exist
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM reputation__endorsements \
         WHERE endorser_id = $1 AND subject_id = $2 AND revoked_at IS NULL",
    )
    .bind(actor.id)
    .bind(subject.id)
    .fetch_one(&pool)
    .await
    .expect("count endorsements");
    assert_eq!(count, 1, "endorsement should exist after endorse action");

    // Weight should match the payload value
    let weight: f32 = sqlx::query_scalar(
        "SELECT weight FROM reputation__endorsements \
         WHERE endorser_id = $1 AND subject_id = $2",
    )
    .bind(actor.id)
    .bind(subject.id)
    .fetch_one(&pool)
    .await
    .expect("fetch weight");
    assert!(
        (weight - 0.8).abs() < 0.001,
        "weight should be 0.8, got {weight}"
    );
}

// ---------------------------------------------------------------------------
// Test 2: revoke action — revokes an existing endorsement
// ---------------------------------------------------------------------------
#[shared_runtime_test]
async fn test_process_batch_revoke_action() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let actor = AccountFactory::new()
        .with_seed(1)
        .create(&pool)
        .await
        .expect("create actor");

    let subject = AccountFactory::new()
        .with_seed(2)
        .create(&pool)
        .await
        .expect("create subject");

    // Create an active endorsement directly
    sqlx::query(
        "INSERT INTO reputation__endorsements (endorser_id, subject_id, topic, weight) \
         VALUES ($1, $2, 'trust', 1.0)",
    )
    .bind(actor.id)
    .bind(subject.id)
    .execute(&pool)
    .await
    .expect("seed endorsement");

    // Enqueue a 'revoke' action
    let trust_repo = PgTrustRepo::new(pool.clone());
    trust_repo
        .enqueue_action(actor.id, "revoke", &json!({ "subject_id": subject.id }))
        .await
        .expect("enqueue revoke action");

    let worker = make_worker(pool.clone());
    let processed = worker.process_one().await.expect("process_one");
    assert!(processed, "expected a message to be processed");

    // Action should be marked completed
    let (status,): (String,) =
        sqlx::query_as("SELECT status FROM trust__action_log WHERE actor_id = $1")
            .bind(actor.id)
            .fetch_one(&pool)
            .await
            .expect("fetch action");
    assert_eq!(status, "completed");

    // Endorsement should now be revoked
    let revoked_at: Option<chrono::DateTime<chrono::Utc>> = sqlx::query_scalar(
        "SELECT revoked_at FROM reputation__endorsements \
         WHERE endorser_id = $1 AND subject_id = $2",
    )
    .bind(actor.id)
    .bind(subject.id)
    .fetch_one(&pool)
    .await
    .expect("fetch endorsement");
    assert!(
        revoked_at.is_some(),
        "endorsement should be revoked (revoked_at IS NOT NULL)"
    );
}

// ---------------------------------------------------------------------------
// Test 3: denounce action — creates a denouncement row
// ---------------------------------------------------------------------------
#[shared_runtime_test]
async fn test_process_batch_denounce_action() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let actor = AccountFactory::new()
        .with_seed(1)
        .create(&pool)
        .await
        .expect("create actor");

    let target = AccountFactory::new()
        .with_seed(2)
        .create(&pool)
        .await
        .expect("create target");

    // Seed influence for the actor so create_denouncement doesn't fail on FK/budget
    sqlx::query(
        "INSERT INTO trust__user_influence (user_id, total_influence) VALUES ($1, 100.0) \
         ON CONFLICT DO NOTHING",
    )
    .bind(actor.id)
    .execute(&pool)
    .await
    .expect("seed influence");

    // Enqueue a 'denounce' action
    let trust_repo = PgTrustRepo::new(pool.clone());
    trust_repo
        .enqueue_action(
            actor.id,
            "denounce",
            &json!({ "target_id": target.id, "reason": "spam", "influence_cost": 1.0 }),
        )
        .await
        .expect("enqueue denounce action");

    let worker = make_worker(pool.clone());
    let processed = worker.process_one().await.expect("process_one");
    assert!(processed, "expected a message to be processed");

    // Action should be marked completed
    let (status,): (String,) =
        sqlx::query_as("SELECT status FROM trust__action_log WHERE actor_id = $1")
            .bind(actor.id)
            .fetch_one(&pool)
            .await
            .expect("fetch action");
    assert_eq!(status, "completed");

    // Denouncement row should exist
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM trust__denouncements WHERE accuser_id = $1 AND target_id = $2",
    )
    .bind(actor.id)
    .bind(target.id)
    .fetch_one(&pool)
    .await
    .expect("count denouncements");
    assert_eq!(
        count, 1,
        "denouncement row should exist after denounce action"
    );
}

// ---------------------------------------------------------------------------
// Test 4: invalid payload causes action to be marked failed
// ---------------------------------------------------------------------------
#[shared_runtime_test]
async fn test_process_batch_invalid_payload_fails() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let actor = AccountFactory::new()
        .with_seed(1)
        .create(&pool)
        .await
        .expect("create actor");

    // Enqueue a 'revoke' action with an invalid payload (missing subject_id)
    let trust_repo = PgTrustRepo::new(pool.clone());
    trust_repo
        .enqueue_action(actor.id, "revoke", &json!({ "not_subject_id": "garbage" }))
        .await
        .expect("enqueue bad action");

    let worker = make_worker(pool.clone());
    let processed = worker.process_one().await.expect("process_one");
    assert!(processed, "expected a message to be processed");

    // Action should be marked failed with an error message
    let (status, error_message): (String, Option<String>) =
        sqlx::query_as("SELECT status, error_message FROM trust__action_log WHERE actor_id = $1")
            .bind(actor.id)
            .fetch_one(&pool)
            .await
            .expect("fetch action");

    assert_eq!(status, "failed");
    assert!(
        error_message.is_some(),
        "error_message should be set for a failed action"
    );
}

// ---------------------------------------------------------------------------
// Test 5: endorse action with out-of-range weight fails the action
// ---------------------------------------------------------------------------
#[shared_runtime_test]
async fn test_process_batch_endorse_invalid_weight_fails() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let actor = AccountFactory::new()
        .with_seed(1)
        .create(&pool)
        .await
        .expect("create actor");

    let subject = AccountFactory::new()
        .with_seed(2)
        .create(&pool)
        .await
        .expect("create subject");

    // Enqueue an 'endorse' action with weight=1.5 (valid range is (0.0, 1.0])
    let trust_repo = PgTrustRepo::new(pool.clone());
    trust_repo
        .enqueue_action(
            actor.id,
            "endorse",
            &json!({ "subject_id": subject.id, "weight": 1.5, "attestation": null, "in_slot": true }),
        )
        .await
        .expect("enqueue action");

    let worker = make_worker(pool.clone());
    let processed = worker.process_one().await.expect("process_one");
    assert!(processed, "expected a message to be processed");

    // Action should be marked failed with an error mentioning weight
    let (status, error_message): (String, Option<String>) =
        sqlx::query_as("SELECT status, error_message FROM trust__action_log WHERE actor_id = $1")
            .bind(actor.id)
            .fetch_one(&pool)
            .await
            .expect("fetch action");

    assert_eq!(status, "failed");
    assert!(
        error_message.as_deref().unwrap_or("").contains("weight"),
        "error_message should mention 'weight', got: {error_message:?}"
    );
}

// ---------------------------------------------------------------------------
// Test 6: endorse action with missing in_slot fails the action
// ---------------------------------------------------------------------------
#[shared_runtime_test]
async fn test_process_batch_endorse_missing_in_slot_fails() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let actor = AccountFactory::new()
        .with_seed(1)
        .create(&pool)
        .await
        .expect("create actor");

    let subject = AccountFactory::new()
        .with_seed(2)
        .create(&pool)
        .await
        .expect("create subject");

    // Enqueue an 'endorse' action without the required 'in_slot' field
    let trust_repo = PgTrustRepo::new(pool.clone());
    trust_repo
        .enqueue_action(
            actor.id,
            "endorse",
            &json!({ "subject_id": subject.id, "weight": 0.5, "attestation": null }),
        )
        .await
        .expect("enqueue action");

    let worker = make_worker(pool.clone());
    let processed = worker.process_one().await.expect("process_one");
    assert!(processed, "expected a message to be processed");

    // Action should be marked failed with an error mentioning in_slot
    let (status, error_message): (String, Option<String>) =
        sqlx::query_as("SELECT status, error_message FROM trust__action_log WHERE actor_id = $1")
            .bind(actor.id)
            .fetch_one(&pool)
            .await
            .expect("fetch action");

    assert_eq!(status, "failed");
    assert!(
        error_message.as_deref().unwrap_or("").contains("in_slot"),
        "error_message should mention 'in_slot', got: {error_message:?}"
    );
}

// ---------------------------------------------------------------------------
// Test 7: denounce action with reason too long fails the action
// ---------------------------------------------------------------------------
#[shared_runtime_test]
async fn test_process_batch_denounce_reason_too_long_fails() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let actor = AccountFactory::new()
        .with_seed(1)
        .create(&pool)
        .await
        .expect("create actor");

    let target = AccountFactory::new()
        .with_seed(2)
        .create(&pool)
        .await
        .expect("create target");

    // Enqueue a 'denounce' action with a reason that is 501 characters (upper bound is 500)
    let trust_repo = PgTrustRepo::new(pool.clone());
    trust_repo
        .enqueue_action(
            actor.id,
            "denounce",
            &json!({ "target_id": target.id, "reason": "a".repeat(501) }),
        )
        .await
        .expect("enqueue action");

    let worker = make_worker(pool.clone());
    let processed = worker.process_one().await.expect("process_one");
    assert!(processed, "expected a message to be processed");

    // Action should be marked failed with an error mentioning reason
    let (status, error_message): (String, Option<String>) =
        sqlx::query_as("SELECT status, error_message FROM trust__action_log WHERE actor_id = $1")
            .bind(actor.id)
            .fetch_one(&pool)
            .await
            .expect("fetch action");

    assert_eq!(status, "failed");
    assert!(
        error_message.as_deref().unwrap_or("").contains("reason"),
        "error_message should mention 'reason', got: {error_message:?}"
    );
}

// ---------------------------------------------------------------------------
// Test 8: empty queue — process_one returns false without blocking
// ---------------------------------------------------------------------------
#[shared_runtime_test]
async fn test_process_one_returns_false_on_empty_queue() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    // Do not enqueue anything — the pgmq queue should be empty.
    let worker = make_worker(pool.clone());
    let processed = worker.process_one().await.expect("process_one");
    assert!(
        !processed,
        "expected process_one to return false when the queue is empty"
    );
}

// ---------------------------------------------------------------------------
// Test 9: poison message — action is marked failed and pgmq message archived
// ---------------------------------------------------------------------------
#[shared_runtime_test]
async fn test_process_one_poison_message_marks_action_failed() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let actor = AccountFactory::new()
        .with_seed(1)
        .create(&pool)
        .await
        .expect("create actor");

    // Enqueue any well-formed action — the poison-message guard fires before
    // the payload is inspected, so the content doesn't need to be valid.
    let trust_repo = PgTrustRepo::new(pool.clone());
    trust_repo
        .enqueue_action(actor.id, "endorse", &json!({}))
        .await
        .expect("enqueue action");

    // Simulate a message that has been delivered many times (e.g., the worker
    // crashed mid-flight each time) by bumping read_ct directly in the pgmq
    // queue table.  After pgmq.read increments read_ct by 1, the worker sees
    // read_ct = 11 > MAX_RETRIES (3) and takes the poison-message path.
    sqlx::query("UPDATE pgmq.q_trust__actions SET read_ct = 10")
        .execute(&pool)
        .await
        .expect("bump read_ct past MAX_RETRIES");

    let worker = make_worker(pool.clone());
    let processed = worker.process_one().await.expect("process_one");
    assert!(processed, "expected poison message to be processed");

    // Action should be marked failed with 'poison message' in error_message.
    let (status, error_message): (String, Option<String>) =
        sqlx::query_as("SELECT status, error_message FROM trust__action_log WHERE actor_id = $1")
            .bind(actor.id)
            .fetch_one(&pool)
            .await
            .expect("fetch action");

    assert_eq!(status, "failed");
    assert!(
        error_message.as_deref().unwrap_or("").contains("poison"),
        "error_message should mention 'poison', got: {error_message:?}"
    );

    // The message should have been archived — the active queue is now empty.
    let queue_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM pgmq.q_trust__actions")
        .fetch_one(&pool)
        .await
        .expect("count active queue");
    assert_eq!(
        queue_count, 0,
        "pgmq queue should be empty after poison message archived"
    );
}

#[shared_runtime_test]
async fn test_process_batch_denounce_empty_reason_fails() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let actor = AccountFactory::new()
        .with_seed(1)
        .create(&pool)
        .await
        .expect("create actor");

    let target = AccountFactory::new()
        .with_seed(2)
        .create(&pool)
        .await
        .expect("create target");

    // Enqueue a 'denounce' action with an empty reason (valid range is 1-500 chars)
    let trust_repo = PgTrustRepo::new(pool.clone());
    trust_repo
        .enqueue_action(
            actor.id,
            "denounce",
            &json!({ "target_id": target.id, "reason": "" }),
        )
        .await
        .expect("enqueue action");

    let worker = make_worker(pool.clone());
    let processed = worker.process_one().await.expect("process_one");
    assert!(processed, "expected a message to be processed");

    // Action should be marked failed with an error mentioning reason
    let (status, error_message): (String, Option<String>) =
        sqlx::query_as("SELECT status, error_message FROM trust__action_log WHERE actor_id = $1")
            .bind(actor.id)
            .fetch_one(&pool)
            .await
            .expect("fetch action");

    assert_eq!(status, "failed");
    assert!(
        error_message.as_deref().unwrap_or("").contains("reason"),
        "error_message should mention 'reason', got: {error_message:?}"
    );
}
