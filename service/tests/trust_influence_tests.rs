//! Integration tests for trust influence repository operations.

mod common;

use common::factories::AccountFactory;
use common::test_db::isolated_db;
use tc_test_macros::shared_runtime_test;
use tinycongress_api::trust::repo::{PgTrustRepo, TrustRepo, TrustRepoError};

#[shared_runtime_test]
async fn test_get_or_create_influence_creates_default() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let account = AccountFactory::new()
        .with_seed(10)
        .create(&pool)
        .await
        .expect("create account");

    let repo = PgTrustRepo::new(pool);
    let record = repo
        .get_or_create_influence(account.id)
        .await
        .expect("get_or_create_influence");

    assert_eq!(record.user_id, account.id);
    assert!((record.total_influence - 10.0).abs() < f32::EPSILON);
    assert!((record.staked_influence - 0.0).abs() < f32::EPSILON);
    assert!((record.spent_influence - 0.0).abs() < f32::EPSILON);
}

#[shared_runtime_test]
async fn test_get_or_create_influence_is_idempotent() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let account = AccountFactory::new()
        .with_seed(21)
        .create(&pool)
        .await
        .expect("create account");

    let repo = PgTrustRepo::new(pool);

    let first = repo
        .get_or_create_influence(account.id)
        .await
        .expect("first call");
    let second = repo
        .get_or_create_influence(account.id)
        .await
        .expect("second call");

    assert_eq!(first.user_id, second.user_id);
    assert!((first.total_influence - second.total_influence).abs() < f32::EPSILON);
    assert!((first.staked_influence - second.staked_influence).abs() < f32::EPSILON);
    assert!((first.spent_influence - second.spent_influence).abs() < f32::EPSILON);
}

#[shared_runtime_test]
async fn test_update_influence_staking() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let account = AccountFactory::new()
        .with_seed(22)
        .create(&pool)
        .await
        .expect("create account");

    let repo = PgTrustRepo::new(pool);

    repo.get_or_create_influence(account.id)
        .await
        .expect("get_or_create");

    let updated = repo
        .update_influence(account.id, 3.0, 0.0)
        .await
        .expect("stake 3.0");

    assert!((updated.staked_influence - 3.0).abs() < f32::EPSILON);
    assert!((updated.spent_influence - 0.0).abs() < f32::EPSILON);
    assert!((updated.total_influence - 10.0).abs() < f32::EPSILON);
}

#[shared_runtime_test]
async fn test_update_influence_spending() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let account = AccountFactory::new()
        .with_seed(23)
        .create(&pool)
        .await
        .expect("create account");

    let repo = PgTrustRepo::new(pool);

    repo.get_or_create_influence(account.id)
        .await
        .expect("get_or_create");

    let updated = repo
        .update_influence(account.id, 0.0, 2.0)
        .await
        .expect("spend 2.0");

    assert!((updated.spent_influence - 2.0).abs() < f32::EPSILON);
    assert!((updated.staked_influence - 0.0).abs() < f32::EPSILON);
}

#[shared_runtime_test]
async fn test_update_influence_rejects_overstake() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let account = AccountFactory::new()
        .with_seed(24)
        .create(&pool)
        .await
        .expect("create account");

    let repo = PgTrustRepo::new(pool);

    repo.get_or_create_influence(account.id)
        .await
        .expect("get_or_create");

    // Try to stake 15.0 when total is only 10.0
    let result = repo.update_influence(account.id, 15.0, 0.0).await;

    assert!(
        matches!(result, Err(TrustRepoError::InsufficientBudget)),
        "expected InsufficientBudget, got: {result:?}"
    );
}

#[shared_runtime_test]
async fn test_update_influence_allows_negative_delta_to_release_stake() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let account = AccountFactory::new()
        .with_seed(25)
        .create(&pool)
        .await
        .expect("create account");

    let repo = PgTrustRepo::new(pool);

    repo.get_or_create_influence(account.id)
        .await
        .expect("get_or_create");

    // Stake 3.0
    repo.update_influence(account.id, 3.0, 0.0)
        .await
        .expect("stake 3.0");

    // Release by staking -3.0
    let released = repo
        .update_influence(account.id, -3.0, 0.0)
        .await
        .expect("release stake");

    assert!((released.staked_influence - 0.0).abs() < f32::EPSILON);
}
