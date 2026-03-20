//! Integration tests for TrustService action orchestration.

mod common;

use std::sync::Arc;

use common::factories::AccountFactory;
use common::test_db::isolated_db;
use tc_test_macros::shared_runtime_test;
use tinycongress_api::reputation::repo::{PgReputationRepo, ReputationRepo};
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
    let rep_repo = Arc::new(PgReputationRepo::new(pool.clone())) as Arc<dyn ReputationRepo>;
    let service = DefaultTrustService::new(repo.clone(), rep_repo);

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

    let rep_repo = Arc::new(PgReputationRepo::new(pool.clone())) as Arc<dyn ReputationRepo>;
    let repo = Arc::new(PgTrustRepo::new(pool));
    let service = DefaultTrustService::new(repo, rep_repo);

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

    let rep_repo = Arc::new(PgReputationRepo::new(pool.clone())) as Arc<dyn ReputationRepo>;
    let repo = Arc::new(PgTrustRepo::new(pool));
    let payload = serde_json::json!({});

    // Enqueue 5 actions directly to hit the daily quota
    for _ in 0..5 {
        repo.enqueue_action(endorser.id, "endorse", &payload)
            .await
            .expect("enqueue action");
    }

    let service = DefaultTrustService::new(repo, rep_repo);
    let result = service.endorse(endorser.id, subject.id, 1.0, None).await;

    assert!(
        matches!(result, Err(TrustServiceError::QuotaExceeded)),
        "expected QuotaExceeded, got: {result:?}"
    );
}

#[shared_runtime_test]
async fn test_self_denounce_rejected() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let user = AccountFactory::new()
        .with_seed(255)
        .create(&pool)
        .await
        .expect("create user");

    let rep_repo = Arc::new(PgReputationRepo::new(pool.clone())) as Arc<dyn ReputationRepo>;
    let repo = Arc::new(PgTrustRepo::new(pool));
    let service = DefaultTrustService::new(repo, rep_repo);

    let result = service.denounce(user.id, user.id, "self-report").await;

    assert!(
        matches!(result, Err(TrustServiceError::SelfAction)),
        "expected SelfAction, got: {result:?}"
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

    let rep_repo = Arc::new(PgReputationRepo::new(pool.clone())) as Arc<dyn ReputationRepo>;
    let repo = Arc::new(PgTrustRepo::new(pool));
    let service = DefaultTrustService::new(repo.clone(), rep_repo);

    service
        .denounce(accuser.id, target.id, "spam")
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

    let rep_repo = Arc::new(PgReputationRepo::new(pool.clone())) as Arc<dyn ReputationRepo>;
    let repo = Arc::new(PgTrustRepo::new(pool));

    // Create 2 denouncements directly via repo (fills d=2 permanent slots)
    repo.create_denouncement(accuser.id, target1.id, "reason")
        .await
        .expect("denouncement 1");
    repo.create_denouncement(accuser.id, target2.id, "reason")
        .await
        .expect("denouncement 2");

    let service = DefaultTrustService::new(repo, rep_repo);
    let result = service.denounce(accuser.id, target3.id, "spam").await;

    assert!(
        matches!(
            result,
            Err(TrustServiceError::DenouncementSlotsExhausted { max: 2 })
        ),
        "expected DenouncementSlotsExhausted, got: {result:?}"
    );
}

#[shared_runtime_test]
async fn test_self_revoke_rejected() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let user = AccountFactory::new()
        .with_seed(217)
        .create(&pool)
        .await
        .expect("create user");

    let rep_repo = Arc::new(PgReputationRepo::new(pool.clone())) as Arc<dyn ReputationRepo>;
    let repo = Arc::new(PgTrustRepo::new(pool));
    let service = DefaultTrustService::new(repo, rep_repo);

    let result = service.revoke_endorsement(user.id, user.id).await;

    assert!(
        matches!(result, Err(TrustServiceError::SelfAction)),
        "expected SelfAction, got: {result:?}"
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

    let rep_repo = Arc::new(PgReputationRepo::new(pool.clone())) as Arc<dyn ReputationRepo>;
    let repo = Arc::new(PgTrustRepo::new(pool));
    let service = DefaultTrustService::new(repo.clone(), rep_repo);

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

#[shared_runtime_test]
async fn test_endorsement_slots_exhausted() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let endorser = AccountFactory::new()
        .with_seed(213)
        .create(&pool)
        .await
        .expect("create endorser");

    // Create 3 subjects and seed endorsements to fill all k=3 slots
    let mut subjects = Vec::new();
    for seed in 214..217 {
        let s = AccountFactory::new()
            .with_seed(seed)
            .create(&pool)
            .await
            .expect("create subject");
        subjects.push(s);
    }

    // Seed 3 active endorsements directly in the DB
    for subject in &subjects {
        sqlx::query(
            "INSERT INTO reputation__endorsements (endorser_id, subject_id, topic, weight) \
             VALUES ($1, $2, 'trust', 1.0)",
        )
        .bind(endorser.id)
        .bind(subject.id)
        .execute(&pool)
        .await
        .expect("seed endorsement");
    }

    // Create a 4th subject to try endorsing
    let extra_subject = AccountFactory::new()
        .with_seed(220)
        .create(&pool)
        .await
        .expect("create extra subject");

    let rep_repo = Arc::new(PgReputationRepo::new(pool.clone())) as Arc<dyn ReputationRepo>;
    let repo = Arc::new(PgTrustRepo::new(pool));
    let service = DefaultTrustService::new(repo, rep_repo);

    // 4th endorsement should succeed (no longer errors) — stored as out-of-slot
    service
        .endorse(endorser.id, extra_subject.id, 1.0, None)
        .await
        .expect("4th endorsement should succeed as out-of-slot");
}

#[shared_runtime_test]
async fn test_revoke_frees_endorsement_slot() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let endorser = AccountFactory::new()
        .with_seed(221)
        .create(&pool)
        .await
        .expect("create endorser");

    // Create 3 subjects and seed endorsements to fill all k=3 slots
    let mut subjects = Vec::new();
    for seed in 222..225 {
        let s = AccountFactory::new()
            .with_seed(seed)
            .create(&pool)
            .await
            .expect("create subject");
        subjects.push(s);
    }

    for subject in &subjects {
        sqlx::query(
            "INSERT INTO reputation__endorsements (endorser_id, subject_id, topic, weight) \
             VALUES ($1, $2, 'trust', 1.0)",
        )
        .bind(endorser.id)
        .bind(subject.id)
        .execute(&pool)
        .await
        .expect("seed endorsement");
    }

    // Revoke one endorsement to free a slot
    sqlx::query(
        "UPDATE reputation__endorsements SET revoked_at = NOW() \
         WHERE endorser_id = $1 AND subject_id = $2 AND topic = 'trust'",
    )
    .bind(endorser.id)
    .bind(subjects[0].id)
    .execute(&pool)
    .await
    .expect("revoke endorsement");

    // Now endorsing a new subject should succeed
    let new_subject = AccountFactory::new()
        .with_seed(228)
        .create(&pool)
        .await
        .expect("create new subject");

    let rep_repo = Arc::new(PgReputationRepo::new(pool.clone())) as Arc<dyn ReputationRepo>;
    let repo = Arc::new(PgTrustRepo::new(pool));
    let service = DefaultTrustService::new(repo, rep_repo);

    service
        .endorse(endorser.id, new_subject.id, 1.0, None)
        .await
        .expect("endorse should succeed after revocation frees a slot");
}

#[shared_runtime_test]
async fn test_endorse_beyond_slot_limit_succeeds() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let endorser = AccountFactory::new()
        .with_seed(250)
        .create(&pool)
        .await
        .expect("create endorser");

    // Create 3 subjects and fill all k=3 slots
    let mut subjects = Vec::new();
    for seed in 251..254 {
        let s = AccountFactory::new()
            .with_seed(seed)
            .create(&pool)
            .await
            .expect("create subject");
        subjects.push(s);
    }

    for subject in &subjects {
        sqlx::query(
            "INSERT INTO reputation__endorsements (endorser_id, subject_id, topic, weight, in_slot) \
             VALUES ($1, $2, 'trust', 1.0, true)",
        )
        .bind(endorser.id)
        .bind(subject.id)
        .execute(&pool)
        .await
        .expect("seed endorsement");
    }

    // 4th endorsement should succeed (not error) but be out-of-slot
    let extra_subject = AccountFactory::new()
        .with_seed(254)
        .create(&pool)
        .await
        .expect("create extra subject");

    let rep_repo = Arc::new(PgReputationRepo::new(pool.clone())) as Arc<dyn ReputationRepo>;
    let repo = Arc::new(PgTrustRepo::new(pool.clone()));
    let service = DefaultTrustService::new(repo, rep_repo);

    service
        .endorse(endorser.id, extra_subject.id, 1.0, None)
        .await
        .expect("4th endorsement should succeed as out-of-slot");
}

#[shared_runtime_test]
async fn test_out_of_slot_endorsement_not_counted_in_budget() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let endorser = AccountFactory::new()
        .with_seed(240)
        .create(&pool)
        .await
        .expect("create endorser");

    let subject = AccountFactory::new()
        .with_seed(241)
        .create(&pool)
        .await
        .expect("create subject");

    // Insert an out-of-slot endorsement directly
    sqlx::query(
        "INSERT INTO reputation__endorsements (endorser_id, subject_id, topic, weight, in_slot) \
         VALUES ($1, $2, 'trust', 1.0, false)",
    )
    .bind(endorser.id)
    .bind(subject.id)
    .execute(&pool)
    .await
    .expect("seed out-of-slot endorsement");

    let rep_repo = PgReputationRepo::new(pool.clone());
    let count = rep_repo
        .count_active_trust_endorsements_by(endorser.id)
        .await
        .expect("count");

    // Out-of-slot endorsement should NOT be counted
    assert_eq!(
        count, 0,
        "out-of-slot endorsement should not count toward budget"
    );
}

#[shared_runtime_test]
async fn test_verifier_bypasses_endorsement_slots() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let verifier = AccountFactory::new()
        .with_seed(230)
        .create(&pool)
        .await
        .expect("create verifier");

    // Give the verifier the authorized_verifier endorsement
    sqlx::query(
        "INSERT INTO reputation__endorsements (subject_id, topic, weight) \
         VALUES ($1, 'authorized_verifier', 1.0)",
    )
    .bind(verifier.id)
    .execute(&pool)
    .await
    .expect("seed verifier endorsement");

    // Create 3 subjects and fill all k=3 slots
    let mut subjects = Vec::new();
    for seed in 231..234 {
        let s = AccountFactory::new()
            .with_seed(seed)
            .create(&pool)
            .await
            .expect("create subject");
        subjects.push(s);
    }

    for subject in &subjects {
        sqlx::query(
            "INSERT INTO reputation__endorsements (endorser_id, subject_id, topic, weight) \
             VALUES ($1, $2, 'trust', 1.0)",
        )
        .bind(verifier.id)
        .bind(subject.id)
        .execute(&pool)
        .await
        .expect("seed endorsement");
    }

    // Verifier should still be able to endorse beyond k=3
    let extra_subject = AccountFactory::new()
        .with_seed(237)
        .create(&pool)
        .await
        .expect("create extra subject");

    let rep_repo = Arc::new(PgReputationRepo::new(pool.clone())) as Arc<dyn ReputationRepo>;
    let repo = Arc::new(PgTrustRepo::new(pool));
    let service = DefaultTrustService::new(repo, rep_repo);

    service
        .endorse(verifier.id, extra_subject.id, 1.0, None)
        .await
        .expect("verifier should bypass endorsement slot limit");
}

#[shared_runtime_test]
async fn test_endorse_rejects_invalid_weight() {
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

    let rep_repo = Arc::new(PgReputationRepo::new(pool.clone())) as Arc<dyn ReputationRepo>;
    let repo = Arc::new(PgTrustRepo::new(pool));
    let service = DefaultTrustService::new(repo, rep_repo);

    for bad_weight in [
        0.0f32,
        -0.5,
        1.1,
        f32::NAN,
        f32::INFINITY,
        f32::NEG_INFINITY,
    ] {
        let result = service
            .endorse(endorser.id, subject.id, bad_weight, None)
            .await;
        assert!(
            matches!(result, Err(TrustServiceError::InvalidWeight)),
            "expected InvalidWeight for weight={bad_weight}, got: {result:?}"
        );
    }
}

#[shared_runtime_test]
async fn test_denounce_rejects_duplicate_denouncement() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let accuser = AccountFactory::new()
        .with_seed(214)
        .create(&pool)
        .await
        .expect("create accuser");

    let target = AccountFactory::new()
        .with_seed(215)
        .create(&pool)
        .await
        .expect("create target");

    let rep_repo = Arc::new(PgReputationRepo::new(pool.clone())) as Arc<dyn ReputationRepo>;
    let repo = Arc::new(PgTrustRepo::new(pool.clone()));

    // Seed an active denouncement directly (simulates a processed worker action).
    repo.create_denouncement(accuser.id, target.id, "initial reason")
        .await
        .expect("create initial denouncement");

    let service = DefaultTrustService::new(repo, rep_repo);
    let result = service
        .denounce(accuser.id, target.id, "duplicate reason")
        .await;

    assert!(
        matches!(result, Err(TrustServiceError::AlreadyDenounced)),
        "expected AlreadyDenounced, got: {result:?}"
    );
}

#[shared_runtime_test]
async fn test_denounce_rejects_empty_reason() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let accuser = AccountFactory::new()
        .with_seed(243)
        .create(&pool)
        .await
        .expect("create accuser");

    let target = AccountFactory::new()
        .with_seed(244)
        .create(&pool)
        .await
        .expect("create target");

    let rep_repo = Arc::new(PgReputationRepo::new(pool.clone())) as Arc<dyn ReputationRepo>;
    let repo = Arc::new(PgTrustRepo::new(pool));
    let service = DefaultTrustService::new(repo, rep_repo);

    let result = service.denounce(accuser.id, target.id, "").await;

    assert!(
        matches!(result, Err(TrustServiceError::InvalidReason { .. })),
        "expected InvalidReason, got: {result:?}"
    );
}

#[shared_runtime_test]
async fn test_denounce_rejects_oversized_reason() {
    use tinycongress_api::trust::service::DENOUNCEMENT_REASON_MAX_LEN;

    let db = isolated_db().await;
    let pool = db.pool().clone();

    let accuser = AccountFactory::new()
        .with_seed(245)
        .create(&pool)
        .await
        .expect("create accuser");

    let target = AccountFactory::new()
        .with_seed(246)
        .create(&pool)
        .await
        .expect("create target");

    let rep_repo = Arc::new(PgReputationRepo::new(pool.clone())) as Arc<dyn ReputationRepo>;
    let repo = Arc::new(PgTrustRepo::new(pool));
    let service = DefaultTrustService::new(repo, rep_repo);

    let oversized = "x".repeat(DENOUNCEMENT_REASON_MAX_LEN + 1);
    let result = service.denounce(accuser.id, target.id, &oversized).await;

    assert!(
        matches!(result, Err(TrustServiceError::InvalidReason { .. })),
        "expected InvalidReason, got: {result:?}"
    );
}

#[shared_runtime_test]
async fn test_denounce_accepts_reason_at_max_length() {
    use tinycongress_api::trust::service::DENOUNCEMENT_REASON_MAX_LEN;

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

    let rep_repo = Arc::new(PgReputationRepo::new(pool.clone())) as Arc<dyn ReputationRepo>;
    let repo = Arc::new(PgTrustRepo::new(pool));
    let service = DefaultTrustService::new(repo, rep_repo);

    let at_limit = "x".repeat(DENOUNCEMENT_REASON_MAX_LEN);
    let result = service.denounce(accuser.id, target.id, &at_limit).await;

    assert!(
        result.is_ok(),
        "expected Ok for reason at max length, got: {result:?}"
    );
}

#[shared_runtime_test]
async fn test_revoke_self_action_rejected() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let user = AccountFactory::new()
        .with_seed(10)
        .create(&pool)
        .await
        .expect("create user");

    let rep_repo = Arc::new(PgReputationRepo::new(pool.clone())) as Arc<dyn ReputationRepo>;
    let repo = Arc::new(PgTrustRepo::new(pool));
    let service = DefaultTrustService::new(repo, rep_repo);

    let result = service.revoke_endorsement(user.id, user.id).await;

    assert!(
        matches!(result, Err(TrustServiceError::SelfAction)),
        "expected SelfAction, got: {result:?}"
    );
}

#[shared_runtime_test]
async fn test_revoke_quota_exceeded() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let endorser = AccountFactory::new()
        .with_seed(11)
        .create(&pool)
        .await
        .expect("create endorser");

    let subject = AccountFactory::new()
        .with_seed(12)
        .create(&pool)
        .await
        .expect("create subject");

    let rep_repo = Arc::new(PgReputationRepo::new(pool.clone())) as Arc<dyn ReputationRepo>;
    let repo = Arc::new(PgTrustRepo::new(pool));
    let payload = serde_json::json!({});

    // Enqueue 5 actions directly to hit the daily quota
    for _ in 0..5 {
        repo.enqueue_action(endorser.id, "revoke", &payload)
            .await
            .expect("enqueue action");
    }

    let service = DefaultTrustService::new(repo, rep_repo);
    let result = service.revoke_endorsement(endorser.id, subject.id).await;

    assert!(
        matches!(result, Err(TrustServiceError::QuotaExceeded)),
        "expected QuotaExceeded, got: {result:?}"
    );
}

#[shared_runtime_test]
async fn test_endorse_rejected_after_denouncement() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let accuser = AccountFactory::new()
        .with_seed(247)
        .create(&pool)
        .await
        .expect("create accuser");

    let target = AccountFactory::new()
        .with_seed(248)
        .create(&pool)
        .await
        .expect("create target");

    let rep_repo = Arc::new(PgReputationRepo::new(pool.clone())) as Arc<dyn ReputationRepo>;
    let repo = Arc::new(PgTrustRepo::new(pool.clone()));

    // Insert a denouncement directly so has_active_denouncement returns true
    repo.create_denouncement(accuser.id, target.id, "prior misbehavior")
        .await
        .expect("create denouncement");

    let service = DefaultTrustService::new(repo, rep_repo);
    let result = service.endorse(accuser.id, target.id, 1.0, None).await;

    assert!(
        matches!(result, Err(TrustServiceError::DenouncementConflict)),
        "expected DenouncementConflict, got: {result:?}"
    );
}

#[shared_runtime_test]
async fn test_revoke_quota_exceeded() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let endorser = AccountFactory::new()
        .with_seed(10)
        .create(&pool)
        .await
        .expect("create endorser");

    let subject = AccountFactory::new()
        .with_seed(11)
        .create(&pool)
        .await
        .expect("create subject");

    let rep_repo = Arc::new(PgReputationRepo::new(pool.clone())) as Arc<dyn ReputationRepo>;
    let repo = Arc::new(PgTrustRepo::new(pool));
    let payload = serde_json::json!({});

    // Enqueue 5 actions directly to hit the daily quota
    for _ in 0..5 {
        repo.enqueue_action(endorser.id, "revoke", &payload)
            .await
            .expect("enqueue action");
    }

    let service = DefaultTrustService::new(repo, rep_repo);
    let result = service.revoke_endorsement(endorser.id, subject.id).await;

    assert!(
        matches!(result, Err(TrustServiceError::QuotaExceeded)),
        "expected QuotaExceeded for revoke_endorsement, got: {result:?}"
    );
}

#[shared_runtime_test]
async fn test_denounce_quota_exceeded() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let accuser = AccountFactory::new()
        .with_seed(12)
        .create(&pool)
        .await
        .expect("create accuser");

    let target = AccountFactory::new()
        .with_seed(13)
        .create(&pool)
        .await
        .expect("create target");

    let rep_repo = Arc::new(PgReputationRepo::new(pool.clone())) as Arc<dyn ReputationRepo>;
    let repo = Arc::new(PgTrustRepo::new(pool));
    let payload = serde_json::json!({});

    // Enqueue 5 actions directly to hit the daily quota
    for _ in 0..5 {
        repo.enqueue_action(accuser.id, "denounce", &payload)
            .await
            .expect("enqueue action");
    }

    let service = DefaultTrustService::new(repo, rep_repo);
    let result = service.denounce(accuser.id, target.id, "spam").await;

    assert!(
        matches!(result, Err(TrustServiceError::QuotaExceeded)),
        "expected QuotaExceeded for denounce, got: {result:?}"
    );
}
