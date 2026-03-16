//! Integration tests for trust invite repository operations.

mod common;

use chrono::Duration;
use common::factories::AccountFactory;
use common::test_db::isolated_db;
use tc_test_macros::shared_runtime_test;
use tinycongress_api::trust::repo::{PgTrustRepo, TrustRepo, TrustRepoError};

#[shared_runtime_test]
async fn test_create_and_get_invite() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let endorser = AccountFactory::new()
        .with_seed(60)
        .create(&pool)
        .await
        .expect("create endorser");

    let repo = PgTrustRepo::new(pool);
    let envelope = vec![1u8, 2, 3, 4];
    let attestation = serde_json::json!({"claim": "trusted"});
    let expires_at = chrono::Utc::now() + Duration::hours(24);

    let created = repo
        .create_invite(
            endorser.id,
            &envelope,
            "qr",
            None,
            1.0,
            &attestation,
            expires_at,
        )
        .await
        .expect("create_invite");

    assert_eq!(created.endorser_id, endorser.id);
    assert_eq!(created.envelope, envelope);
    assert_eq!(created.delivery_method, "qr");
    assert_eq!(created.attestation, attestation);
    assert!(created.accepted_by.is_none());
    assert!(created.accepted_at.is_none());

    let fetched = repo.get_invite(created.id).await.expect("get_invite");

    assert_eq!(fetched.id, created.id);
    assert_eq!(fetched.endorser_id, endorser.id);
}

#[shared_runtime_test]
async fn test_accept_invite() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let endorser = AccountFactory::new()
        .with_seed(61)
        .create(&pool)
        .await
        .expect("create endorser");

    let acceptor = AccountFactory::new()
        .with_seed(62)
        .create(&pool)
        .await
        .expect("create acceptor");

    let repo = PgTrustRepo::new(pool);
    let attestation = serde_json::json!({});
    let expires_at = chrono::Utc::now() + Duration::hours(24);

    let invite = repo
        .create_invite(
            endorser.id,
            &[0u8],
            "email",
            None,
            1.0,
            &attestation,
            expires_at,
        )
        .await
        .expect("create_invite");

    let accepted = repo
        .accept_invite(invite.id, acceptor.id)
        .await
        .expect("accept_invite");

    assert_eq!(accepted.accepted_by, Some(acceptor.id));
    assert!(accepted.accepted_at.is_some());
}

#[shared_runtime_test]
async fn test_accept_already_accepted_invite_rejected() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let endorser = AccountFactory::new()
        .with_seed(63)
        .create(&pool)
        .await
        .expect("create endorser");

    let acceptor1 = AccountFactory::new()
        .with_seed(64)
        .create(&pool)
        .await
        .expect("create acceptor1");

    let acceptor2 = AccountFactory::new()
        .with_seed(65)
        .create(&pool)
        .await
        .expect("create acceptor2");

    let repo = PgTrustRepo::new(pool);
    let attestation = serde_json::json!({});
    let expires_at = chrono::Utc::now() + Duration::hours(24);

    let invite = repo
        .create_invite(
            endorser.id,
            &[0u8],
            "qr",
            None,
            1.0,
            &attestation,
            expires_at,
        )
        .await
        .expect("create_invite");

    repo.accept_invite(invite.id, acceptor1.id)
        .await
        .expect("first accept");

    let result = repo.accept_invite(invite.id, acceptor2.id).await;

    assert!(
        matches!(result, Err(TrustRepoError::NotFound)),
        "expected NotFound on double-accept, got: {result:?}"
    );
}

#[shared_runtime_test]
async fn test_accept_expired_invite_rejected() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let endorser = AccountFactory::new()
        .with_seed(66)
        .create(&pool)
        .await
        .expect("create endorser");

    let acceptor = AccountFactory::new()
        .with_seed(67)
        .create(&pool)
        .await
        .expect("create acceptor");

    let repo = PgTrustRepo::new(pool);
    let attestation = serde_json::json!({});
    // Create an already-expired invite
    let expires_at = chrono::Utc::now() - Duration::hours(1);

    let invite = repo
        .create_invite(
            endorser.id,
            &[0u8],
            "qr",
            None,
            1.0,
            &attestation,
            expires_at,
        )
        .await
        .expect("create_invite");

    let result = repo.accept_invite(invite.id, acceptor.id).await;

    assert!(
        matches!(result, Err(TrustRepoError::NotFound)),
        "expected NotFound for expired invite, got: {result:?}"
    );
}

#[shared_runtime_test]
async fn test_list_invites_by_endorser() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let endorser = AccountFactory::new()
        .with_seed(68)
        .create(&pool)
        .await
        .expect("create endorser");

    let repo = PgTrustRepo::new(pool);
    let attestation = serde_json::json!({});
    let expires_at = chrono::Utc::now() + Duration::hours(24);

    repo.create_invite(
        endorser.id,
        &[1u8],
        "qr",
        None,
        1.0,
        &attestation,
        expires_at,
    )
    .await
    .expect("invite 1");
    repo.create_invite(
        endorser.id,
        &[2u8],
        "email",
        None,
        1.0,
        &attestation,
        expires_at,
    )
    .await
    .expect("invite 2");

    let list = repo
        .list_invites_by_endorser(endorser.id)
        .await
        .expect("list_invites_by_endorser");

    assert_eq!(list.len(), 2);
    assert!(list.iter().all(|i| i.endorser_id == endorser.id));
}

#[shared_runtime_test]
async fn test_invite_stores_weight_and_relationship_depth() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let endorser = AccountFactory::new()
        .with_seed(69)
        .create(&pool)
        .await
        .expect("create endorser");

    let repo = PgTrustRepo::new(pool);
    let attestation = serde_json::json!({});
    let expires_at = chrono::Utc::now() + Duration::hours(24);

    // video + months: 0.7 * 0.7 = 0.49
    let invite = repo
        .create_invite(
            endorser.id,
            &[0u8],
            "video",
            Some("months"),
            0.49,
            &attestation,
            expires_at,
        )
        .await
        .expect("create_invite with weight");

    assert_eq!(invite.delivery_method, "video");
    assert_eq!(invite.relationship_depth.as_deref(), Some("months"));
    assert!(
        (invite.weight - 0.49).abs() < 0.001,
        "expected weight ~0.49, got {}",
        invite.weight
    );
}

#[shared_runtime_test]
async fn test_invite_weight_defaults_to_one() {
    let db = isolated_db().await;
    let pool = db.pool().clone();

    let endorser = AccountFactory::new()
        .with_seed(70)
        .create(&pool)
        .await
        .expect("create endorser");

    let repo = PgTrustRepo::new(pool);
    let attestation = serde_json::json!({});
    let expires_at = chrono::Utc::now() + Duration::hours(24);

    let invite = repo
        .create_invite(
            endorser.id,
            &[0u8],
            "qr",
            None,
            1.0,
            &attestation,
            expires_at,
        )
        .await
        .expect("create_invite default weight");

    assert!(
        (invite.weight - 1.0).abs() < f32::EPSILON,
        "expected weight 1.0, got {}",
        invite.weight
    );
    assert!(invite.relationship_depth.is_none());
}
