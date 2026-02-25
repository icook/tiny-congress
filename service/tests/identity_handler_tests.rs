//! Identity handler integration tests -- signup flow with real DB.

mod common;

use axum::{
    body::{to_bytes, Body},
    http::{header::CONTENT_TYPE, Method, Request, StatusCode},
};
use common::app_builder::TestAppBuilder;
use common::test_db::isolated_db;
use ed25519_dalek::{Signer, SigningKey};
use rand::rngs::OsRng;
use tc_crypto::{encode_base64url, BackupEnvelope};
use tc_test_macros::shared_runtime_test;
use tower::ServiceExt;

/// Build a valid signup JSON body with real Ed25519 keys and certificate.
fn valid_signup_json(username: &str) -> String {
    let root_signing_key = SigningKey::generate(&mut OsRng);
    let root_pubkey_bytes = root_signing_key.verifying_key().to_bytes();
    let root_pubkey = encode_base64url(&root_pubkey_bytes);

    let device_signing_key = SigningKey::generate(&mut OsRng);
    let device_pubkey_bytes = device_signing_key.verifying_key().to_bytes();
    let device_pubkey = encode_base64url(&device_pubkey_bytes);

    let certificate_sig = root_signing_key.sign(&device_pubkey_bytes);
    let certificate = encode_base64url(&certificate_sig.to_bytes());

    let envelope = BackupEnvelope::build(
        [0xAA; 16],  // salt
        65536, 3, 1, // m_cost, t_cost, p_cost
        [0xBB; 12],  // nonce
        &[0xCC; 48], // ciphertext
    )
    .expect("test envelope");
    let backup_blob = encode_base64url(envelope.as_bytes());

    format!(
        r#"{{"username": "{username}", "root_pubkey": "{root_pubkey}", "backup": {{"encrypted_blob": "{backup_blob}"}}, "device": {{"pubkey": "{device_pubkey}", "name": "Test Device", "certificate": "{certificate}"}}}}"#
    )
}

#[shared_runtime_test]
async fn test_signup_handler_success() {
    let db = isolated_db().await;
    let app = TestAppBuilder::new()
        .with_identity_pool(db.pool().clone())
        .build();

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/auth/signup")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(valid_signup_json("signuptest")))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::CREATED);

    let body = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let body_str = String::from_utf8(body.to_vec()).expect("utf8");
    assert!(body_str.contains("account_id"));
    assert!(body_str.contains("root_kid"));
    assert!(body_str.contains("device_kid"));
}

#[shared_runtime_test]
async fn test_signup_handler_duplicate_username() {
    let db = isolated_db().await;

    // First signup succeeds
    let app = TestAppBuilder::new()
        .with_identity_pool(db.pool().clone())
        .build();

    let body = valid_signup_json("dupuser");
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/auth/signup")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(body))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::CREATED);

    // Second signup with same username fails
    let app = TestAppBuilder::new()
        .with_identity_pool(db.pool().clone())
        .build();

    let body = valid_signup_json("dupuser");
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/auth/signup")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(body))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::CONFLICT);

    let body = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let body_str = String::from_utf8(body.to_vec()).expect("utf8");
    assert!(body_str.contains("Username already taken"));
}
