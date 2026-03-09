//! Integration tests for trust denouncement repository operations.

mod common;

use common::factories::AccountFactory;
use common::test_db::isolated_db;
use tc_test_macros::shared_runtime_test;
use tinycongress_api::trust::repo::{PgTrustRepo, TrustRepo, TrustRepoError};

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
        .create_denouncement(accuser.id, target.id, "spam behavior", 2.5)
        .await
        .expect("create_denouncement");

    assert_eq!(record.accuser_id, accuser.id);
    assert_eq!(record.target_id, target.id);
    assert_eq!(record.reason, "spam behavior");
    assert!((record.influence_spent - 2.5).abs() < f32::EPSILON);
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
    repo.create_denouncement(accuser.id, target.id, "first", 1.0)
        .await
        .expect("first denouncement");

    let result = repo
        .create_denouncement(accuser.id, target.id, "second", 1.0)
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
    repo.create_denouncement(accuser1.id, target.id, "reason one", 1.0)
        .await
        .expect("denouncement 1");
    repo.create_denouncement(accuser2.id, target.id, "reason two", 2.0)
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
    repo.create_denouncement(accuser.id, target1.id, "reason", 1.0)
        .await
        .expect("denouncement 1");
    repo.create_denouncement(accuser.id, target2.id, "reason", 1.0)
        .await
        .expect("denouncement 2");

    let count = repo
        .count_active_denouncements_by(accuser.id)
        .await
        .expect("count_active_denouncements_by");

    assert_eq!(count, 2);
}
