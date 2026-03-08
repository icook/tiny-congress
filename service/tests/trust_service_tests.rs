//! Integration tests for TrustService action orchestration.

mod common;

use std::sync::Arc;

use common::factories::AccountFactory;
use common::test_db::isolated_db;
use tc_test_macros::shared_runtime_test;
use tinycongress_api::trust::repo::{PgTrustRepo, TrustRepo};
use tinycongress_api::trust::service::{DefaultTrustService, TrustService, TrustServiceError};

#[shared_runtime_test]
async fn test_endorse_enqueues_action() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let endorser = AccountFactory::new()
        .with_seed(200)
        .create(&pool)
        .await
        .expect("create endorser");

    let subject = AccountFactory::new()
        .with_seed(201)
        .create(&pool)
        .await
        .expect("create subject");

    let repo = Arc::new(PgTrustRepo::new(pool.clone()));
    let service = DefaultTrustService::new(repo.clone());

    service
        .endorse(endorser.id, subject.id, 1.0, None)
        .await
        .expect("endorse");

    let count = repo
        .count_daily_actions(endorser.id)
        .await
        .expect("count_daily_actions");

    assert_eq!(count, 1);
}

#[shared_runtime_test]
async fn test_self_endorse_rejected() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let user = AccountFactory::new()
        .with_seed(202)
        .create(&pool)
        .await
        .expect("create user");

    let repo = Arc::new(PgTrustRepo::new(pool));
    let service = DefaultTrustService::new(repo);

    let result = service.endorse(user.id, user.id, 1.0, None).await;

    assert!(
        matches!(result, Err(TrustServiceError::SelfAction)),
        "expected SelfAction, got: {result:?}"
    );
}

#[shared_runtime_test]
async fn test_daily_quota_exceeded() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let endorser = AccountFactory::new()
        .with_seed(203)
        .create(&pool)
        .await
        .expect("create endorser");

    let subject = AccountFactory::new()
        .with_seed(204)
        .create(&pool)
        .await
        .expect("create subject");

    let repo = Arc::new(PgTrustRepo::new(pool));
    let payload = serde_json::json!({});

    // Enqueue 5 actions directly to hit the daily quota
    for _ in 0..5 {
        repo.enqueue_action(endorser.id, "endorse", &payload)
            .await
            .expect("enqueue action");
    }

    let service = DefaultTrustService::new(repo);
    let result = service.endorse(endorser.id, subject.id, 1.0, None).await;

    assert!(
        matches!(result, Err(TrustServiceError::QuotaExceeded)),
        "expected QuotaExceeded, got: {result:?}"
    );
}

#[shared_runtime_test]
async fn test_denounce_enqueues_action() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let accuser = AccountFactory::new()
        .with_seed(205)
        .create(&pool)
        .await
        .expect("create accuser");

    let target = AccountFactory::new()
        .with_seed(206)
        .create(&pool)
        .await
        .expect("create target");

    // Seed influence so the accuser can afford the denouncement
    sqlx::query(
        "INSERT INTO trust__user_influence (user_id, total_influence) VALUES ($1, 100.0) \
         ON CONFLICT DO NOTHING",
    )
    .bind(accuser.id)
    .execute(&pool)
    .await
    .expect("seed influence");

    let repo = Arc::new(PgTrustRepo::new(pool));
    let service = DefaultTrustService::new(repo.clone());

    service
        .denounce(accuser.id, target.id, "spam", 1.0)
        .await
        .expect("denounce");

    let count = repo
        .count_daily_actions(accuser.id)
        .await
        .expect("count_daily_actions");

    assert_eq!(count, 1);
}

#[shared_runtime_test]
async fn test_denounce_slots_exhausted() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let accuser = AccountFactory::new()
        .with_seed(207)
        .create(&pool)
        .await
        .expect("create accuser");

    let target1 = AccountFactory::new()
        .with_seed(208)
        .create(&pool)
        .await
        .expect("create target1");

    let target2 = AccountFactory::new()
        .with_seed(209)
        .create(&pool)
        .await
        .expect("create target2");

    let target3 = AccountFactory::new()
        .with_seed(210)
        .create(&pool)
        .await
        .expect("create target3");

    // Seed influence for accuser
    sqlx::query(
        "INSERT INTO trust__user_influence (user_id, total_influence) VALUES ($1, 100.0) \
         ON CONFLICT DO NOTHING",
    )
    .bind(accuser.id)
    .execute(&pool)
    .await
    .expect("seed influence");

    let repo = Arc::new(PgTrustRepo::new(pool));

    // Create 2 denouncements directly via repo (fills d=2 slots)
    repo.create_denouncement(accuser.id, target1.id, "reason", 1.0)
        .await
        .expect("denouncement 1");
    repo.create_denouncement(accuser.id, target2.id, "reason", 1.0)
        .await
        .expect("denouncement 2");

    let service = DefaultTrustService::new(repo);
    let result = service.denounce(accuser.id, target3.id, "spam", 1.0).await;

    assert!(
        matches!(
            result,
            Err(TrustServiceError::DenouncementSlotsExhausted { max: 2 })
        ),
        "expected DenouncementSlotsExhausted, got: {result:?}"
    );
}

#[shared_runtime_test]
async fn test_revoke_enqueues_action() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let endorser = AccountFactory::new()
        .with_seed(211)
        .create(&pool)
        .await
        .expect("create endorser");

    let subject = AccountFactory::new()
        .with_seed(212)
        .create(&pool)
        .await
        .expect("create subject");

    let repo = Arc::new(PgTrustRepo::new(pool));
    let service = DefaultTrustService::new(repo.clone());

    service
        .revoke_endorsement(endorser.id, subject.id)
        .await
        .expect("revoke_endorsement");

    let count = repo
        .count_daily_actions(endorser.id)
        .await
        .expect("count_daily_actions");

    assert_eq!(count, 1);
}
