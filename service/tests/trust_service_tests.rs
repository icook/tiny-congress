//! Integration tests for TrustService action orchestration.

mod common;

use std::sync::Arc;

use common::factories::AccountFactory;
use common::test_db::isolated_db;
use tc_test_macros::shared_runtime_test;
use tinycongress_api::reputation::repo::{PgReputationRepo, ReputationRepo};
use tinycongress_api::trust::repo::{PgTrustRepo, TrustRepo};
use tinycongress_api::trust::service::{
    ActionType, DefaultTrustService, TrustService, TrustServiceError,
};

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
        repo.enqueue_action(endorser.id, ActionType::Endorse, &payload)
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

/// Verifier accounts are exempt from endorsement slot limits and their
/// endorsements should always be queued with `in_slot=true`, even when
/// all k=3 slots are already occupied.
///
/// `test_verifier_bypasses_endorsement_slots` already checks the service
/// doesn't error; this test pins the payload value so a logic inversion
/// (accidentally setting `in_slot=false` for verifiers) fails loudly.
#[shared_runtime_test]
async fn test_verifier_endorse_beyond_slots_queues_in_slot_true() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let verifier = AccountFactory::new()
        .with_seed(235)
        .create(&pool)
        .await
        .expect("create verifier");

    // Grant the authorized_verifier endorsement (genesis — no endorser_id).
    sqlx::query(
        "INSERT INTO reputation__endorsements (subject_id, topic, weight) \
         VALUES ($1, 'authorized_verifier', 1.0)",
    )
    .bind(verifier.id)
    .execute(&pool)
    .await
    .expect("seed verifier endorsement");

    // Fill all k=3 slots with in-slot endorsements.
    for seed in 56u8..59 {
        let s = AccountFactory::new()
            .with_seed(seed)
            .create(&pool)
            .await
            .expect("create subject");
        sqlx::query(
            "INSERT INTO reputation__endorsements \
             (endorser_id, subject_id, topic, weight, in_slot) \
             VALUES ($1, $2, 'trust', 1.0, true)",
        )
        .bind(verifier.id)
        .bind(s.id)
        .execute(&pool)
        .await
        .expect("seed in-slot endorsement");
    }

    let extra_subject = AccountFactory::new()
        .with_seed(59)
        .create(&pool)
        .await
        .expect("create extra subject");

    let rep_repo = Arc::new(PgReputationRepo::new(pool.clone())) as Arc<dyn ReputationRepo>;
    let repo = Arc::new(PgTrustRepo::new(pool.clone()));
    let service = DefaultTrustService::new(repo, rep_repo);

    service
        .endorse(verifier.id, extra_subject.id, 1.0, None)
        .await
        .expect("verifier should bypass slot limit");

    // Retrieve the queued action and assert in_slot=true.
    let row: (serde_json::Value,) = sqlx::query_as(
        "SELECT payload FROM trust__action_log \
         WHERE actor_id = $1 AND action_type = 'endorse' \
         ORDER BY created_at DESC LIMIT 1",
    )
    .bind(verifier.id)
    .fetch_one(&pool)
    .await
    .expect("action should exist");

    assert_eq!(
        row.0["in_slot"].as_bool(),
        Some(true),
        "verifier endorsement beyond slot limit must queue in_slot=true"
    );
}

/// Non-verifier accounts with all k=3 slots occupied must queue the action
/// with `in_slot=false`, so the endorsement does NOT count in the trust graph.
///
/// This mirrors `test_verifier_endorse_beyond_slots_queues_in_slot_true` and
/// pins the opposite branch: a logic inversion (setting `in_slot=true` for a
/// non-verifier out-of-slot endorsement) would silently inflate trust scores.
#[shared_runtime_test]
async fn test_non_verifier_endorse_beyond_slots_queues_in_slot_false() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let endorser = AccountFactory::new()
        .with_seed(62)
        .create(&pool)
        .await
        .expect("create endorser");

    // Fill all k=3 slots with in-slot endorsements (no authorized_verifier grant).
    for seed in 63u8..66 {
        let s = AccountFactory::new()
            .with_seed(seed)
            .create(&pool)
            .await
            .expect("create subject");
        sqlx::query(
            "INSERT INTO reputation__endorsements \
             (endorser_id, subject_id, topic, weight, in_slot) \
             VALUES ($1, $2, 'trust', 1.0, true)",
        )
        .bind(endorser.id)
        .bind(s.id)
        .execute(&pool)
        .await
        .expect("seed in-slot endorsement");
    }

    let extra_subject = AccountFactory::new()
        .with_seed(66)
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

    // Retrieve the queued action and assert in_slot=false.
    let row: (serde_json::Value,) = sqlx::query_as(
        "SELECT payload FROM trust__action_log \
         WHERE actor_id = $1 AND action_type = 'endorse' \
         ORDER BY created_at DESC LIMIT 1",
    )
    .bind(endorser.id)
    .fetch_one(&pool)
    .await
    .expect("action should exist");

    assert_eq!(
        row.0["in_slot"].as_bool(),
        Some(false),
        "non-verifier endorsement beyond slot limit must queue in_slot=false"
    );
}
