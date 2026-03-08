//! Integration tests for TrustWorker batch processing.

mod common;

use std::sync::Arc;

use common::factories::AccountFactory;
use common::test_db::isolated_db;
use serde_json::json;
use tc_test_macros::shared_runtime_test;
use tinycongress_api::reputation::repo::PgReputationRepo;
use tinycongress_api::trust::engine::TrustEngine;
use tinycongress_api::trust::repo::PgTrustRepo;
use tinycongress_api::trust::worker::TrustWorker;

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

    // Seed an 'endorse' action directly into the queue
    sqlx::query(
        "INSERT INTO trust__action_queue (actor_id, action_type, payload) VALUES ($1, $2, $3)",
    )
    .bind(actor.id)
    .bind("endorse")
    .bind(json!({ "subject_id": subject.id, "weight": 0.8, "attestation": null }))
    .execute(&pool)
    .await
    .expect("seed action");

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
    assert_eq!(processed, 1);

    // Action should be marked completed
    let (status,): (String,) =
        sqlx::query_as("SELECT status FROM trust__action_queue WHERE actor_id = $1")
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

    // Seed a 'revoke' action
    sqlx::query(
        "INSERT INTO trust__action_queue (actor_id, action_type, payload) VALUES ($1, $2, $3)",
    )
    .bind(actor.id)
    .bind("revoke")
    .bind(json!({ "subject_id": subject.id }))
    .execute(&pool)
    .await
    .expect("seed revoke action");

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
    assert_eq!(processed, 1);

    // Action should be marked completed
    let (status,): (String,) =
        sqlx::query_as("SELECT status FROM trust__action_queue WHERE actor_id = $1")
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

    // Seed a 'denounce' action
    sqlx::query(
        "INSERT INTO trust__action_queue (actor_id, action_type, payload) VALUES ($1, $2, $3)",
    )
    .bind(actor.id)
    .bind("denounce")
    .bind(json!({ "target_id": target.id, "reason": "spam", "influence_cost": 1.0 }))
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
    assert_eq!(processed, 1);

    // Action should be marked completed
    let (status,): (String,) =
        sqlx::query_as("SELECT status FROM trust__action_queue WHERE actor_id = $1")
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

    // Seed a 'revoke' action with an invalid payload (missing subject_id)
    // — this exercises the fail path without needing to bypass the action_type CHECK constraint.
    sqlx::query(
        "INSERT INTO trust__action_queue (actor_id, action_type, payload) VALUES ($1, $2, $3)",
    )
    .bind(actor.id)
    .bind("revoke")
    .bind(json!({ "not_subject_id": "garbage" }))
    .execute(&pool)
    .await
    .expect("seed bad action");

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
    assert_eq!(processed, 1);

    // Action should be marked failed with an error message
    let (status, error_message): (String, Option<String>) =
        sqlx::query_as("SELECT status, error_message FROM trust__action_queue WHERE actor_id = $1")
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
