//! Integration tests for TrustWorker pgmq-based processing.

mod common;

use std::sync::Arc;

use common::factories::AccountFactory;
use common::test_db::isolated_db;
use serde_json::json;
use tc_engine_polling::repo::pgmq;
use tc_test_macros::shared_runtime_test;
use tinycongress_api::reputation::repo::PgReputationRepo;
use tinycongress_api::trust::engine::TrustEngine;
use tinycongress_api::trust::repo::{PgTrustRepo, TrustRepo};
use tinycongress_api::trust::service::ActionType;
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
            ActionType::Endorse,
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
        .enqueue_action(
            actor.id,
            ActionType::Revoke,
            &json!({ "subject_id": subject.id }),
        )
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
            ActionType::Denounce,
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
        .enqueue_action(
            actor.id,
            ActionType::Revoke,
            &json!({ "not_subject_id": "garbage" }),
        )
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
            ActionType::Endorse,
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
            ActionType::Endorse,
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
            ActionType::Denounce,
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
        .enqueue_action(actor.id, ActionType::Endorse, &json!({}))
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

// ---------------------------------------------------------------------------
// Test: poison message with invalid log_id — archived without fail_action call
// ---------------------------------------------------------------------------

/// When a poison message (read_ct > MAX_RETRIES) contains an invalid or missing
/// log_id, `extract_log_id` returns `None` and `fail_action` is skipped — there
/// is no action record to update. The message must still be archived so it does
/// not become visible again and loop indefinitely.
///
/// This covers the `None` arm of the `if let Some(log_id) = extract_log_id(...)`
/// guard inside the poison-message branch of `process_one`.
#[shared_runtime_test]
async fn test_process_one_poison_message_with_invalid_log_id_is_archived() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    // Send a message with an invalid log_id directly — no trust__action_log entry
    // exists. This simulates a corrupt or manually-injected pgmq message.
    pgmq::send(&pool, "trust__actions", &json!({ "log_id": "not-a-uuid" }))
        .await
        .expect("send bad message");

    // Simulate repeated delivery failures: bump read_ct so the worker sees
    // read_ct = 11 > MAX_RETRIES (3) and takes the poison-message path.
    sqlx::query("UPDATE pgmq.q_trust__actions SET read_ct = 10")
        .execute(&pool)
        .await
        .expect("bump read_ct past MAX_RETRIES");

    let worker = make_worker(pool.clone());
    let processed = worker.process_one().await.expect("process_one");
    assert!(processed, "expected poison message to be processed");

    // The message must be archived — the active queue is now empty.
    // fail_action was not called (no valid log_id to reference), but the
    // worker must not panic or leave the message visible.
    let queue_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM pgmq.q_trust__actions")
        .fetch_one(&pool)
        .await
        .expect("count active queue");
    assert_eq!(
        queue_count, 0,
        "pgmq queue should be empty after corrupt poison message archived"
    );
}

// ---------------------------------------------------------------------------
// Test: denounce action revokes an existing endorsement atomically
// ---------------------------------------------------------------------------
#[shared_runtime_test]
async fn test_process_batch_denounce_revokes_existing_endorsement() {
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

    // Seed an active trust endorsement from actor → target
    sqlx::query(
        "INSERT INTO reputation__endorsements (endorser_id, subject_id, topic, weight) \
         VALUES ($1, $2, 'trust', 1.0)",
    )
    .bind(actor.id)
    .bind(target.id)
    .execute(&pool)
    .await
    .expect("seed endorsement");

    // Seed influence for the actor so create_denouncement doesn't fail
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
            ActionType::Denounce,
            &json!({ "target_id": target.id, "reason": "spam" }),
        )
        .await
        .expect("enqueue denounce action");

    let worker = make_worker(pool.clone());
    let processed = worker.process_one().await.expect("process_one");
    assert!(processed, "expected a message to be processed");

    // Action should be completed
    let (status,): (String,) =
        sqlx::query_as("SELECT status FROM trust__action_log WHERE actor_id = $1")
            .bind(actor.id)
            .fetch_one(&pool)
            .await
            .expect("fetch action");
    assert_eq!(status, "completed");

    // Denouncement row should exist
    let denounce_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM trust__denouncements WHERE accuser_id = $1 AND target_id = $2",
    )
    .bind(actor.id)
    .bind(target.id)
    .fetch_one(&pool)
    .await
    .expect("count denouncements");
    assert_eq!(denounce_count, 1, "denouncement row should exist");

    // The endorsement should now be revoked (revoked_at IS NOT NULL)
    let revoked_at: Option<chrono::DateTime<chrono::Utc>> = sqlx::query_scalar(
        "SELECT revoked_at FROM reputation__endorsements \
         WHERE endorser_id = $1 AND subject_id = $2 AND topic = 'trust'",
    )
    .bind(actor.id)
    .bind(target.id)
    .fetch_one(&pool)
    .await
    .expect("fetch endorsement");
    assert!(
        revoked_at.is_some(),
        "denounce action should atomically revoke any existing endorsement"
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
            ActionType::Denounce,
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

// ---------------------------------------------------------------------------
// Test: orphaned pgmq message — get_action returns NotFound
// ---------------------------------------------------------------------------

/// When a pgmq message references a log_id that no longer exists in
/// `trust__action_log` (e.g. the row was deleted between enqueue and
/// processing), the worker should log the error, leave the message in the
/// queue for retry, and return `true` (a message was consumed from read).
#[shared_runtime_test]
async fn test_process_one_orphaned_message_leaves_queue_and_continues() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let actor = AccountFactory::new()
        .with_seed(1)
        .create(&pool)
        .await
        .expect("create actor");

    // Enqueue an action — inserts into trust__action_log AND sends a pgmq message.
    let trust_repo = PgTrustRepo::new(pool.clone());
    let record = trust_repo
        .enqueue_action(actor.id, ActionType::Endorse, &json!({}))
        .await
        .expect("enqueue action");

    // Delete the action log row, leaving the pgmq message orphaned.
    sqlx::query("DELETE FROM trust__action_log WHERE id = $1")
        .bind(record.id)
        .execute(&pool)
        .await
        .expect("delete action log row");

    let worker = make_worker(pool.clone());
    let processed = worker.process_one().await.expect("process_one");
    assert!(
        processed,
        "expected process_one to return true for an orphaned pgmq message"
    );

    // The message must remain in the queue (invisible) so the visibility
    // timeout will re-expose it for retry — it must NOT be archived.
    let queue_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM pgmq.q_trust__actions")
        .fetch_one(&pool)
        .await
        .expect("count active queue");
    assert_eq!(
        queue_count, 1,
        "orphaned message should remain in queue for retry, not be archived"
    );

    // No action log entries should exist (none were created by the worker).
    let log_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM trust__action_log")
        .fetch_one(&pool)
        .await
        .expect("count action log");
    assert_eq!(
        log_count, 0,
        "orphaned-message handling must not create new action log entries"
    );
}

// ---------------------------------------------------------------------------
// Test 10: denounce action with absent reason field fails the action
// ---------------------------------------------------------------------------
#[shared_runtime_test]
async fn test_process_batch_denounce_missing_reason_field_fails() {
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

    // Enqueue a 'denounce' action without the required 'reason' key
    let trust_repo = PgTrustRepo::new(pool.clone());
    trust_repo
        .enqueue_action(
            actor.id,
            ActionType::Denounce,
            &json!({ "target_id": target.id }),
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
// Test: denounce action with whitespace-only reason fails the action
// ---------------------------------------------------------------------------

/// `is_valid_reason` rejects whitespace-only strings in addition to empty ones.
/// This test confirms the rejection flows end-to-end through the worker so that
/// a payload that bypasses the service layer (e.g. injected directly via
/// `enqueue_action`) cannot persist a denouncement with a blank reason.
#[shared_runtime_test]
async fn test_process_batch_denounce_whitespace_only_reason_fails() {
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

    // Enqueue a 'denounce' action with a whitespace-only reason.
    let trust_repo = PgTrustRepo::new(pool.clone());
    trust_repo
        .enqueue_action(
            actor.id,
            ActionType::Denounce,
            &json!({ "target_id": target.id, "reason": "   " }),
        )
        .await
        .expect("enqueue action");

    let worker = make_worker(pool.clone());
    let processed = worker.process_one().await.expect("process_one");
    assert!(processed, "expected a message to be processed");

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
// Test: revoke action with no existing endorsement — completes as no-op
// ---------------------------------------------------------------------------

/// Revoking an endorsement that does not exist must complete successfully.
///
/// `revoke_endorsement` issues a bare UPDATE that affects 0 rows when the
/// endorsement is absent; it does not return `NotFound`.  This test documents
/// that contract so any future change (e.g. returning `NotFound` on zero rows)
/// is caught before it silently starts failing user-initiated revocations.
#[shared_runtime_test]
async fn test_process_batch_revoke_no_endorsement_is_no_op() {
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

    // Enqueue a revoke action without seeding any prior endorsement.
    let trust_repo = PgTrustRepo::new(pool.clone());
    trust_repo
        .enqueue_action(
            actor.id,
            ActionType::Revoke,
            &json!({ "subject_id": subject.id }),
        )
        .await
        .expect("enqueue revoke action");

    let worker = make_worker(pool.clone());
    let processed = worker.process_one().await.expect("process_one");
    assert!(processed, "expected a message to be processed");

    // Action must be marked completed — a no-op revoke is not a failure.
    let (status,): (String,) =
        sqlx::query_as("SELECT status FROM trust__action_log WHERE actor_id = $1")
            .bind(actor.id)
            .fetch_one(&pool)
            .await
            .expect("fetch action");
    assert_eq!(
        status, "completed",
        "revoke action should complete even when no endorsement exists to revoke"
    );

    // No endorsement row should have been created as a side effect.
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM reputation__endorsements \
         WHERE endorser_id = $1 AND subject_id = $2",
    )
    .bind(actor.id)
    .bind(subject.id)
    .fetch_one(&pool)
    .await
    .expect("count endorsements");
    assert_eq!(count, 0, "revoke should not create any endorsement rows");
}

// ---------------------------------------------------------------------------
// Test: denounce action when denouncement already exists fails the action
// ---------------------------------------------------------------------------

/// If a denouncement already exists for the (accuser, target) pair when the
/// worker processes the action — a race condition where two concurrent HTTP
/// requests both pass the `has_active_denouncement` guard before either writes —
/// the unique constraint fires, the action is marked failed, and the
/// pre-existing denouncement is left unchanged.
#[shared_runtime_test]
async fn test_process_batch_denounce_duplicate_denouncement_marks_action_failed() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let actor = AccountFactory::new()
        .with_seed(20)
        .create(&pool)
        .await
        .expect("create actor");

    let target = AccountFactory::new()
        .with_seed(21)
        .create(&pool)
        .await
        .expect("create target");

    // Pre-seed a denouncement so the unique constraint fires when the worker runs.
    sqlx::query(
        "INSERT INTO trust__denouncements (accuser_id, target_id, reason) VALUES ($1, $2, $3)",
    )
    .bind(actor.id)
    .bind(target.id)
    .bind("prior reason")
    .execute(&pool)
    .await
    .expect("seed denouncement");

    // Enqueue a denounce action for the same pair — simulates the second
    // concurrent request that also passed the service's has_active_denouncement check.
    let trust_repo = PgTrustRepo::new(pool.clone());
    trust_repo
        .enqueue_action(
            actor.id,
            ActionType::Denounce,
            &json!({ "target_id": target.id, "reason": "duplicate reason" }),
        )
        .await
        .expect("enqueue denounce action");

    let worker = make_worker(pool.clone());
    let processed = worker.process_one().await.expect("process_one");
    assert!(processed, "expected a message to be processed");

    // Action should be marked failed — the unique constraint fired.
    let (status, error_message): (String, Option<String>) =
        sqlx::query_as("SELECT status, error_message FROM trust__action_log WHERE actor_id = $1")
            .bind(actor.id)
            .fetch_one(&pool)
            .await
            .expect("fetch action");

    assert_eq!(status, "failed");
    assert!(
        error_message.is_some(),
        "error_message should be set when the unique constraint fires"
    );

    // The pre-seeded denouncement should still be the only one — the duplicate was rejected.
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM trust__denouncements WHERE accuser_id = $1 AND target_id = $2",
    )
    .bind(actor.id)
    .bind(target.id)
    .fetch_one(&pool)
    .await
    .expect("count denouncements");
    assert_eq!(count, 1, "only the pre-seeded denouncement should exist");

    // The original reason must be unchanged — the failed duplicate must not overwrite it.
    let reason: String = sqlx::query_scalar(
        "SELECT reason FROM trust__denouncements WHERE accuser_id = $1 AND target_id = $2",
    )
    .bind(actor.id)
    .bind(target.id)
    .fetch_one(&pool)
    .await
    .expect("fetch denouncement reason");
    assert_eq!(
        reason, "prior reason",
        "original denouncement reason must not be overwritten by the failed duplicate"
    );
}

// ---------------------------------------------------------------------------
// Test: action with a subject_id that is a string but not a valid UUID fails
// ---------------------------------------------------------------------------

/// `parse_uuid` has two error branches: key absent (tested elsewhere) and key
/// present as a non-UUID string. This test covers the second branch, confirming
/// the action is marked failed with an error message that names the bad field.
#[shared_runtime_test]
async fn test_process_batch_invalid_uuid_string_fails() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let actor = AccountFactory::new()
        .with_seed(1)
        .create(&pool)
        .await
        .expect("create actor");

    // Enqueue a 'revoke' action with subject_id present as a non-UUID string.
    let trust_repo = PgTrustRepo::new(pool.clone());
    trust_repo
        .enqueue_action(
            actor.id,
            ActionType::Revoke,
            &json!({ "subject_id": "not-a-valid-uuid" }),
        )
        .await
        .expect("enqueue action");

    let worker = make_worker(pool.clone());
    let processed = worker.process_one().await.expect("process_one");
    assert!(processed, "expected a message to be processed");

    let (status, error_message): (String, Option<String>) =
        sqlx::query_as("SELECT status, error_message FROM trust__action_log WHERE actor_id = $1")
            .bind(actor.id)
            .fetch_one(&pool)
            .await
            .expect("fetch action");

    assert_eq!(status, "failed");
    assert!(
        error_message
            .as_deref()
            .unwrap_or("")
            .contains("subject_id"),
        "error_message should name the bad field, got: {error_message:?}"
    );
}
