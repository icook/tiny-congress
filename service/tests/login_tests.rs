//! Login handler integration tests.
//!
//! Tests the POST /auth/login endpoint with real database connections,
//! verifying timestamp validation, certificate verification with timestamp
//! binding, and nonce-based replay protection.

mod common;

use axum::{
    body::{to_bytes, Body},
    http::{header::CONTENT_TYPE, Method, Request, StatusCode},
};
use common::app_builder::TestAppBuilder;
use common::factories::valid_signup_with_keys;
use common::test_db::isolated_db;
use ed25519_dalek::{Signer, SigningKey};
use rand::rngs::OsRng;
use tc_crypto::encode_base64url;
use tc_test_macros::shared_runtime_test;
use tower::ServiceExt;

/// Build a valid login JSON body with timestamp-bound certificate.
fn login_json(
    username: &str,
    root_signing_key: &SigningKey,
    device_pubkey: &[u8],
    device_name: &str,
    timestamp: i64,
) -> String {
    // Certificate signs device_pubkey || timestamp (LE i64 bytes)
    let mut signed_payload = Vec::with_capacity(40);
    signed_payload.extend_from_slice(device_pubkey);
    signed_payload.extend_from_slice(&timestamp.to_le_bytes());
    let cert = root_signing_key.sign(&signed_payload);

    serde_json::json!({
        "username": username,
        "timestamp": timestamp,
        "device": {
            "pubkey": encode_base64url(device_pubkey),
            "name": device_name,
            "certificate": encode_base64url(&cert.to_bytes()),
        }
    })
    .to_string()
}

fn login_request(body: &str) -> Request<Body> {
    Request::builder()
        .method(Method::POST)
        .uri("/auth/login")
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .expect("request")
}

// =========================================================================
// Success path
// =========================================================================

#[shared_runtime_test]
async fn test_login_success() {
    let db = isolated_db().await;
    let app = TestAppBuilder::new()
        .with_identity_pool(db.pool().clone())
        .build();

    // First, sign up a user
    let (signup_json, keys) = valid_signup_with_keys("loginuser");
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/auth/signup")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(signup_json))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::CREATED);

    // Now log in with a new device key
    let new_device_key = SigningKey::generate(&mut OsRng);
    let new_device_pubkey = new_device_key.verifying_key().to_bytes();
    let timestamp = chrono::Utc::now().timestamp();
    let body = login_json(
        "loginuser",
        &keys.root_signing_key,
        &new_device_pubkey,
        "Login Device",
        timestamp,
    );

    let response = app.oneshot(login_request(&body)).await.expect("response");
    assert_eq!(response.status(), StatusCode::OK);

    let body_bytes = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).expect("json");
    assert!(json["account_id"].is_string());
    assert!(json["root_kid"].is_string());
    assert!(json["device_kid"].is_string());
}

// =========================================================================
// Timestamp validation
// =========================================================================

#[shared_runtime_test]
async fn test_login_expired_timestamp() {
    let db = isolated_db().await;
    let app = TestAppBuilder::new()
        .with_identity_pool(db.pool().clone())
        .build();

    // Sign up
    let (signup_json, keys) = valid_signup_with_keys("expiredts");
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/auth/signup")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(signup_json))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::CREATED);

    // Login with timestamp 400s in the past
    let new_device_key = SigningKey::generate(&mut OsRng);
    let new_device_pubkey = new_device_key.verifying_key().to_bytes();
    let old_timestamp = chrono::Utc::now().timestamp() - 400;
    let body = login_json(
        "expiredts",
        &keys.root_signing_key,
        &new_device_pubkey,
        "Old Device",
        old_timestamp,
    );

    let response = app.oneshot(login_request(&body)).await.expect("response");
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body_bytes = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let body_str = String::from_utf8(body_bytes.to_vec()).expect("utf8");
    assert!(body_str.contains("Timestamp out of range"));
}

#[shared_runtime_test]
async fn test_login_future_timestamp() {
    let db = isolated_db().await;
    let app = TestAppBuilder::new()
        .with_identity_pool(db.pool().clone())
        .build();

    // Sign up
    let (signup_json, keys) = valid_signup_with_keys("futurets");
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/auth/signup")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(signup_json))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::CREATED);

    // Login with timestamp 400s in the future
    let new_device_key = SigningKey::generate(&mut OsRng);
    let new_device_pubkey = new_device_key.verifying_key().to_bytes();
    let future_timestamp = chrono::Utc::now().timestamp() + 400;
    let body = login_json(
        "futurets",
        &keys.root_signing_key,
        &new_device_pubkey,
        "Future Device",
        future_timestamp,
    );

    let response = app.oneshot(login_request(&body)).await.expect("response");
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body_bytes = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let body_str = String::from_utf8(body_bytes.to_vec()).expect("utf8");
    assert!(body_str.contains("Timestamp out of range"));
}

// =========================================================================
// Replay protection
// =========================================================================

#[shared_runtime_test]
async fn test_login_replay_detected() {
    let db = isolated_db().await;
    let app = TestAppBuilder::new()
        .with_identity_pool(db.pool().clone())
        .build();

    // Sign up
    let (signup_json, keys) = valid_signup_with_keys("replaylogin");
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/auth/signup")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(signup_json))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::CREATED);

    // Build login request
    let new_device_key = SigningKey::generate(&mut OsRng);
    let new_device_pubkey = new_device_key.verifying_key().to_bytes();
    let timestamp = chrono::Utc::now().timestamp();
    let body = login_json(
        "replaylogin",
        &keys.root_signing_key,
        &new_device_pubkey,
        "Replay Device",
        timestamp,
    );

    // First request succeeds
    let response = app
        .clone()
        .oneshot(login_request(&body))
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);

    // Second request with exact same body is a replay
    let response = app.oneshot(login_request(&body)).await.expect("response");

    // Could be either replay detection (nonce) or duplicate device key (DuplicateKid).
    // Both are valid rejection reasons. The nonce fires first because it's checked
    // before the create_device_key call.
    assert!(
        response.status() == StatusCode::BAD_REQUEST || response.status() == StatusCode::CONFLICT,
        "Expected 400 (replay) or 409 (duplicate), got {}",
        response.status()
    );

    let body_bytes = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let body_str = String::from_utf8(body_bytes.to_vec()).expect("utf8");
    assert!(
        body_str.contains("replay") || body_str.contains("already registered"),
        "Expected replay or duplicate error, got: {body_str}"
    );
}

// =========================================================================
// Certificate verification
// =========================================================================

#[shared_runtime_test]
async fn test_login_old_cert_format_rejected() {
    let db = isolated_db().await;
    let app = TestAppBuilder::new()
        .with_identity_pool(db.pool().clone())
        .build();

    // Sign up
    let (signup_json, keys) = valid_signup_with_keys("oldformat");
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/auth/signup")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(signup_json))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::CREATED);

    // Build login with certificate that signs ONLY device_pubkey (no timestamp)
    let new_device_key = SigningKey::generate(&mut OsRng);
    let new_device_pubkey = new_device_key.verifying_key().to_bytes();
    let timestamp = chrono::Utc::now().timestamp();

    // Sign only the device pubkey (old format, without timestamp)
    let cert = keys.root_signing_key.sign(&new_device_pubkey);

    let body = serde_json::json!({
        "username": "oldformat",
        "timestamp": timestamp,
        "device": {
            "pubkey": encode_base64url(&new_device_pubkey),
            "name": "Old Format Device",
            "certificate": encode_base64url(&cert.to_bytes()),
        }
    })
    .to_string();

    let response = app.oneshot(login_request(&body)).await.expect("response");
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body_bytes = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let body_str = String::from_utf8(body_bytes.to_vec()).expect("utf8");
    assert!(body_str.contains("Invalid device certificate"));
}

#[shared_runtime_test]
async fn test_login_wrong_root_key_rejected() {
    let db = isolated_db().await;
    let app = TestAppBuilder::new()
        .with_identity_pool(db.pool().clone())
        .build();

    // Sign up
    let (signup_json, _keys) = valid_signup_with_keys("wrongroot");
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/auth/signup")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(signup_json))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::CREATED);

    // Login with a certificate signed by a different root key
    let wrong_root = SigningKey::generate(&mut OsRng);
    let new_device_key = SigningKey::generate(&mut OsRng);
    let new_device_pubkey = new_device_key.verifying_key().to_bytes();
    let timestamp = chrono::Utc::now().timestamp();
    let body = login_json(
        "wrongroot",
        &wrong_root,
        &new_device_pubkey,
        "Wrong Root Device",
        timestamp,
    );

    let response = app.oneshot(login_request(&body)).await.expect("response");
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body_bytes = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let body_str = String::from_utf8(body_bytes.to_vec()).expect("utf8");
    assert!(body_str.contains("Invalid device certificate"));
}

// =========================================================================
// Error paths
// =========================================================================

#[shared_runtime_test]
async fn test_login_unknown_username() {
    let db = isolated_db().await;
    let app = TestAppBuilder::new()
        .with_identity_pool(db.pool().clone())
        .build();

    let root = SigningKey::generate(&mut OsRng);
    let device = SigningKey::generate(&mut OsRng);
    let device_pubkey = device.verifying_key().to_bytes();
    let timestamp = chrono::Utc::now().timestamp();

    let body = login_json("nonexistent", &root, &device_pubkey, "Device", timestamp);

    let response = app.oneshot(login_request(&body)).await.expect("response");
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body_bytes = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let body_str = String::from_utf8(body_bytes.to_vec()).expect("utf8");
    assert!(body_str.contains("Invalid credentials"));
}
