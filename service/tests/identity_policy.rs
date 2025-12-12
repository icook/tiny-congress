#![allow(clippy::float_cmp)]

use tinycongress_api::db;
use tinycongress_api::identity::policy::{
    attributes::fetch_attributes,
    evaluator::{authorize, Action},
};
use uuid::Uuid;

#[tokio::test]
async fn test_fetch_attributes_for_account() {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/tinycongress".to_string());
    let pool = db::setup_database(&database_url).await.unwrap();

    sqlx::query("TRUNCATE accounts, devices, device_delegations, reputation_scores CASCADE")
        .execute(&pool)
        .await
        .unwrap();

    let account_id = Uuid::new_v4();
    let device_id = Uuid::new_v4();

    sqlx::query!(
        r#"
        INSERT INTO accounts (id, username, root_kid, root_pubkey, tier, verification_state)
        VALUES ($1, 'testuser', 'test_kid', 'test_pubkey', 'verified', 'verified')
        "#,
        account_id
    )
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query!(
        r#"
        INSERT INTO devices (id, account_id, device_kid, device_pubkey, name, type)
        VALUES ($1, $2, 'device_kid', 'device_pubkey', 'Test Device', 'laptop')
        "#,
        device_id,
        account_id
    )
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query!(
        r#"
        INSERT INTO device_delegations (account_id, device_id, delegation_envelope, issued_at)
        VALUES ($1, $2, '{}'::jsonb, NOW())
        "#,
        account_id,
        device_id
    )
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query!(
        r#"
        INSERT INTO reputation_scores (account_id, score, posture_label)
        VALUES ($1, 75.0, 'strong')
        "#,
        account_id
    )
    .execute(&pool)
    .await
    .unwrap();

    let attrs = fetch_attributes(&pool, account_id, Some(device_id))
        .await
        .unwrap();

    assert_eq!(attrs.account_id, account_id);
    assert_eq!(attrs.device_id, Some(device_id));
    assert_eq!(attrs.tier, "verified");
    assert_eq!(attrs.verification_state, "verified");
    assert_eq!(attrs.reputation_score, 75.0);
    assert_eq!(attrs.posture_label, Some("strong".to_string()));
    assert!(!attrs.device_revoked);
    assert!(attrs.delegation_active);
}

#[tokio::test]
async fn test_fetch_attributes_with_revoked_device() {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/tinycongress".to_string());
    let pool = db::setup_database(&database_url).await.unwrap();

    sqlx::query("TRUNCATE accounts, devices, device_delegations, reputation_scores CASCADE")
        .execute(&pool)
        .await
        .unwrap();

    let account_id = Uuid::new_v4();
    let device_id = Uuid::new_v4();

    sqlx::query!(
        r#"
        INSERT INTO accounts (id, username, root_kid, root_pubkey, tier, verification_state)
        VALUES ($1, 'testuser', 'test_kid', 'test_pubkey', 'verified', 'verified')
        "#,
        account_id
    )
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query!(
        r#"
        INSERT INTO devices (id, account_id, device_kid, device_pubkey, name, type, revoked_at)
        VALUES ($1, $2, 'device_kid', 'device_pubkey', 'Test Device', 'laptop', NOW())
        "#,
        device_id,
        account_id
    )
    .execute(&pool)
    .await
    .unwrap();

    let attrs = fetch_attributes(&pool, account_id, Some(device_id))
        .await
        .unwrap();

    assert!(attrs.device_revoked);
    assert!(!attrs.delegation_active);
}

#[tokio::test]
async fn test_authorization_with_active_device() {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/tinycongress".to_string());
    let pool = db::setup_database(&database_url).await.unwrap();

    sqlx::query("TRUNCATE accounts, devices, device_delegations, reputation_scores CASCADE")
        .execute(&pool)
        .await
        .unwrap();

    let account_id = Uuid::new_v4();
    let device_id = Uuid::new_v4();

    sqlx::query!(
        r#"
        INSERT INTO accounts (id, username, root_kid, root_pubkey, tier, verification_state)
        VALUES ($1, 'testuser', 'test_kid', 'test_pubkey', 'verified', 'verified')
        "#,
        account_id
    )
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query!(
        r#"
        INSERT INTO devices (id, account_id, device_kid, device_pubkey, name, type)
        VALUES ($1, $2, 'device_kid', 'device_pubkey', 'Test Device', 'laptop')
        "#,
        device_id,
        account_id
    )
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query!(
        r#"
        INSERT INTO device_delegations (account_id, device_id, delegation_envelope, issued_at)
        VALUES ($1, $2, '{}'::jsonb, NOW())
        "#,
        account_id,
        device_id
    )
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query!(
        r#"
        INSERT INTO reputation_scores (account_id, score, posture_label)
        VALUES ($1, 50.0, 'ok')
        "#,
        account_id
    )
    .execute(&pool)
    .await
    .unwrap();

    let attrs = fetch_attributes(&pool, account_id, Some(device_id))
        .await
        .unwrap();

    assert!(authorize(&Action::CreateEndorsement, &attrs, None).unwrap());
    assert!(authorize(&Action::AddDevice, &attrs, None).unwrap());
    assert!(authorize(&Action::RotateRoot, &attrs, None).unwrap());
}

#[tokio::test]
async fn test_authorization_fails_with_revoked_device() {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/tinycongress".to_string());
    let pool = db::setup_database(&database_url).await.unwrap();

    sqlx::query("TRUNCATE accounts, devices, device_delegations, reputation_scores CASCADE")
        .execute(&pool)
        .await
        .unwrap();

    let account_id = Uuid::new_v4();
    let device_id = Uuid::new_v4();

    sqlx::query!(
        r#"
        INSERT INTO accounts (id, username, root_kid, root_pubkey, tier, verification_state)
        VALUES ($1, 'testuser', 'test_kid', 'test_pubkey', 'verified', 'verified')
        "#,
        account_id
    )
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query!(
        r#"
        INSERT INTO devices (id, account_id, device_kid, device_pubkey, name, type, revoked_at)
        VALUES ($1, $2, 'device_kid', 'device_pubkey', 'Test Device', 'laptop', NOW())
        "#,
        device_id,
        account_id
    )
    .execute(&pool)
    .await
    .unwrap();

    let attrs = fetch_attributes(&pool, account_id, Some(device_id))
        .await
        .unwrap();

    assert!(!authorize(&Action::CreateEndorsement, &attrs, None).unwrap());
    assert!(!authorize(&Action::RevokeEndorsement, &attrs, None).unwrap());
}

#[tokio::test]
async fn test_authorization_fails_with_low_reputation() {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/tinycongress".to_string());
    let pool = db::setup_database(&database_url).await.unwrap();

    sqlx::query("TRUNCATE accounts, devices, device_delegations, reputation_scores CASCADE")
        .execute(&pool)
        .await
        .unwrap();

    let account_id = Uuid::new_v4();

    sqlx::query!(
        r#"
        INSERT INTO accounts (id, username, root_kid, root_pubkey, tier, verification_state)
        VALUES ($1, 'testuser', 'test_kid', 'test_pubkey', 'verified', 'verified')
        "#,
        account_id
    )
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query!(
        r#"
        INSERT INTO reputation_scores (account_id, score, posture_label)
        VALUES ($1, 5.0, 'weak')
        "#,
        account_id
    )
    .execute(&pool)
    .await
    .unwrap();

    let attrs = fetch_attributes(&pool, account_id, None).await.unwrap();

    assert!(!authorize(&Action::RotateRoot, &attrs, None).unwrap());
}

#[tokio::test]
async fn test_authorization_add_device_requires_verified_tier() {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/tinycongress".to_string());
    let pool = db::setup_database(&database_url).await.unwrap();

    sqlx::query("TRUNCATE accounts, devices, device_delegations, reputation_scores CASCADE")
        .execute(&pool)
        .await
        .unwrap();

    let account_id = Uuid::new_v4();

    sqlx::query!(
        r#"
        INSERT INTO accounts (id, username, root_kid, root_pubkey, tier, verification_state)
        VALUES ($1, 'testuser', 'test_kid', 'test_pubkey', 'anonymous', 'none')
        "#,
        account_id
    )
    .execute(&pool)
    .await
    .unwrap();

    let attrs = fetch_attributes(&pool, account_id, None).await.unwrap();

    assert!(!authorize(&Action::AddDevice, &attrs, None).unwrap());

    sqlx::query!(
        r#"
        UPDATE accounts SET tier = 'verified' WHERE id = $1
        "#,
        account_id
    )
    .execute(&pool)
    .await
    .unwrap();

    let attrs = fetch_attributes(&pool, account_id, None).await.unwrap();
    assert!(authorize(&Action::AddDevice, &attrs, None).unwrap());
}
