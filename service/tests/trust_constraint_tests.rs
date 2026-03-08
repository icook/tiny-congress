//! Integration tests for room constraint trait and preset implementations.

mod common;

use common::factories::AccountFactory;
use common::test_db::isolated_db;
use serde_json::json;
use tc_test_macros::shared_runtime_test;
use tinycongress_api::trust::constraints::{
    build_constraint, CommunityConstraint, CongressConstraint, EndorsedByConstraint, RoomConstraint,
};
use tinycongress_api::trust::repo::{PgTrustRepo, TrustRepo};

// ---------------------------------------------------------------------------
// EndorsedByConstraint: user reachable from anchor → eligible
// ---------------------------------------------------------------------------
#[shared_runtime_test]
async fn test_endorsed_by_eligible() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let anchor = AccountFactory::new()
        .with_seed(80)
        .create(&pool)
        .await
        .expect("create anchor");
    let user = AccountFactory::new()
        .with_seed(81)
        .create(&pool)
        .await
        .expect("create user");

    // Insert endorsement edge and score snapshot
    sqlx::query(
        "INSERT INTO reputation__endorsements (endorser_id, subject_id, topic, weight)
         VALUES ($1, $2, 'trust', $3)",
    )
    .bind(anchor.id)
    .bind(user.id)
    .bind(1.0_f32)
    .execute(&pool)
    .await
    .unwrap();

    let repo = PgTrustRepo::new(pool.clone());
    repo.upsert_score(user.id, Some(anchor.id), Some(1.0), Some(1), None)
        .await
        .expect("upsert_score");

    let constraint = EndorsedByConstraint;
    let result = constraint
        .check(user.id, anchor.id, &repo)
        .await
        .expect("check should not error");

    assert!(
        result.is_eligible,
        "User reachable from anchor should be eligible"
    );
    assert!(result.reason.is_none());
}

// ---------------------------------------------------------------------------
// EndorsedByConstraint: no score snapshot → ineligible with reason
// ---------------------------------------------------------------------------
#[shared_runtime_test]
async fn test_endorsed_by_ineligible() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let anchor = AccountFactory::new()
        .with_seed(82)
        .create(&pool)
        .await
        .expect("create anchor");
    let user = AccountFactory::new()
        .with_seed(83)
        .create(&pool)
        .await
        .expect("create user");

    // No score inserted — user is unreachable
    let repo = PgTrustRepo::new(pool.clone());
    let constraint = EndorsedByConstraint;
    let result = constraint
        .check(user.id, anchor.id, &repo)
        .await
        .expect("check should not error");

    assert!(
        !result.is_eligible,
        "User with no score should be ineligible"
    );
    let reason = result.reason.expect("should have a reason");
    assert!(
        reason.contains("not reachable"),
        "Reason should mention 'not reachable', got: {reason}"
    );
}

// ---------------------------------------------------------------------------
// CommunityConstraint: distance and diversity within limits → eligible
// ---------------------------------------------------------------------------
#[shared_runtime_test]
async fn test_community_eligible() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let anchor = AccountFactory::new()
        .with_seed(84)
        .create(&pool)
        .await
        .expect("create anchor");
    let user = AccountFactory::new()
        .with_seed(85)
        .create(&pool)
        .await
        .expect("create user");

    let repo = PgTrustRepo::new(pool.clone());
    repo.upsert_score(user.id, Some(anchor.id), Some(3.0), Some(3), None)
        .await
        .expect("upsert_score");

    let constraint = CommunityConstraint::new(5.0, 2).unwrap();
    let result = constraint
        .check(user.id, anchor.id, &repo)
        .await
        .expect("check should not error");

    assert!(
        result.is_eligible,
        "User with distance=3.0, diversity=3 should be eligible"
    );
}

// ---------------------------------------------------------------------------
// CommunityConstraint: distance exceeds max → ineligible, reason mentions distance
// ---------------------------------------------------------------------------
#[shared_runtime_test]
async fn test_community_ineligible_distance() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let anchor = AccountFactory::new()
        .with_seed(86)
        .create(&pool)
        .await
        .expect("create anchor");
    let user = AccountFactory::new()
        .with_seed(87)
        .create(&pool)
        .await
        .expect("create user");

    let repo = PgTrustRepo::new(pool.clone());
    repo.upsert_score(user.id, Some(anchor.id), Some(7.0), Some(3), None)
        .await
        .expect("upsert_score");

    let constraint = CommunityConstraint::new(5.0, 2).unwrap();
    let result = constraint
        .check(user.id, anchor.id, &repo)
        .await
        .expect("check should not error");

    assert!(
        !result.is_eligible,
        "User with distance=7.0 > max_distance=5.0 should be ineligible"
    );
    let reason = result.reason.expect("should have a reason");
    assert!(
        reason.contains("distance"),
        "Reason should mention 'distance', got: {reason}"
    );
}

// ---------------------------------------------------------------------------
// CommunityConstraint: diversity below min → ineligible, reason mentions diversity
// ---------------------------------------------------------------------------
#[shared_runtime_test]
async fn test_community_ineligible_diversity() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let anchor = AccountFactory::new()
        .with_seed(88)
        .create(&pool)
        .await
        .expect("create anchor");
    let user = AccountFactory::new()
        .with_seed(89)
        .create(&pool)
        .await
        .expect("create user");

    let repo = PgTrustRepo::new(pool.clone());
    repo.upsert_score(user.id, Some(anchor.id), Some(2.0), Some(1), None)
        .await
        .expect("upsert_score");

    let constraint = CommunityConstraint::new(5.0, 2).unwrap();
    let result = constraint
        .check(user.id, anchor.id, &repo)
        .await
        .expect("check should not error");

    assert!(
        !result.is_eligible,
        "User with diversity=1 < min_diversity=2 should be ineligible"
    );
    let reason = result.reason.expect("should have a reason");
    assert!(
        reason.contains("diversity"),
        "Reason should mention 'diversity', got: {reason}"
    );
}

// ---------------------------------------------------------------------------
// build_constraint factory: known and unknown types
// ---------------------------------------------------------------------------
#[shared_runtime_test]
async fn test_build_constraint_factory() {
    // endorsed_by with empty config → Ok
    let result = build_constraint("endorsed_by", &json!({}));
    assert!(result.is_ok(), "endorsed_by should build successfully");

    // community with max_distance override → Ok
    let result = build_constraint("community", &json!({"max_distance": 4.0}));
    assert!(result.is_ok(), "community should build successfully");

    // congress with default config → Ok
    let result = build_constraint("congress", &json!({}));
    assert!(result.is_ok(), "congress should build successfully");

    // unknown type → Err
    let result = build_constraint("unknown", &json!({}));
    match result {
        Ok(_) => panic!("unknown constraint type should return Err"),
        Err(e) => {
            let msg = e.to_string();
            assert!(
                msg.contains("unknown constraint type"),
                "Error should mention 'unknown constraint type', got: {msg}"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// CommunityConstraint: both distance and diversity fail → ineligible, reason mentions both
// ---------------------------------------------------------------------------
#[shared_runtime_test]
async fn test_community_both_fail() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let anchor = AccountFactory::new()
        .with_seed(90)
        .create(&pool)
        .await
        .expect("create anchor");
    let user = AccountFactory::new()
        .with_seed(91)
        .create(&pool)
        .await
        .expect("create user");

    let repo = PgTrustRepo::new(pool.clone());
    // distance=7.0 exceeds max=5.0; diversity=1 is below min=2
    repo.upsert_score(user.id, Some(anchor.id), Some(7.0), Some(1), None)
        .await
        .expect("upsert_score");

    let constraint = CommunityConstraint::new(5.0, 2).unwrap();
    let result = constraint
        .check(user.id, anchor.id, &repo)
        .await
        .expect("check should not error");

    assert!(
        !result.is_eligible,
        "User with distance=7.0 and diversity=1 should be ineligible"
    );
    let reason = result.reason.expect("should have a reason");
    assert!(
        reason.contains("distance"),
        "Reason should mention 'distance', got: {reason}"
    );
    assert!(
        reason.contains("diversity"),
        "Reason should mention 'diversity', got: {reason}"
    );
}

// ---------------------------------------------------------------------------
// CongressConstraint: path_diversity meets minimum → eligible
// ---------------------------------------------------------------------------
#[shared_runtime_test]
async fn test_congress_eligible() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let anchor = AccountFactory::new()
        .with_seed(92)
        .create(&pool)
        .await
        .expect("create anchor");
    let user = AccountFactory::new()
        .with_seed(93)
        .create(&pool)
        .await
        .expect("create user");

    let repo = PgTrustRepo::new(pool.clone());
    repo.upsert_score(user.id, Some(anchor.id), None, Some(4), None)
        .await
        .expect("upsert_score");

    let constraint = CongressConstraint::new(3).unwrap();
    let result = constraint
        .check(user.id, anchor.id, &repo)
        .await
        .expect("check should not error");

    assert!(
        result.is_eligible,
        "User with path_diversity=4 >= min=3 should be eligible"
    );
    assert!(result.reason.is_none());
}

// ---------------------------------------------------------------------------
// CongressConstraint: no score snapshot → ineligible
// ---------------------------------------------------------------------------
#[shared_runtime_test]
async fn test_congress_ineligible() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let anchor = AccountFactory::new()
        .with_seed(94)
        .create(&pool)
        .await
        .expect("create anchor");
    let user = AccountFactory::new()
        .with_seed(95)
        .create(&pool)
        .await
        .expect("create user");

    // No score inserted — no snapshot for this user
    let repo = PgTrustRepo::new(pool.clone());
    let constraint = CongressConstraint::new(3).unwrap();
    let result = constraint
        .check(user.id, anchor.id, &repo)
        .await
        .expect("check should not error");

    assert!(
        !result.is_eligible,
        "User with no score snapshot should be ineligible"
    );
    assert!(result.reason.is_some(), "Should have a reason");
}
