//! Login handler integration tests.
//!
//! Tests the unauthenticated login flow:
//! - `GET /auth/backup/{username}` — fetch encrypted backup (returns 200 for both
//!   existing and non-existent users to prevent username enumeration)
//! - `POST /auth/login` — authorize a new device via timestamp-bound certificate
//!   with nonce-based replay protection.

mod common;

use axum::{
    body::{to_bytes, Body},
    http::{header::CONTENT_TYPE, Method, Request, StatusCode},
};
use common::app_builder::TestAppBuilder;
use common::factories::{valid_signup_with_keys, SignupKeys};
use common::test_db::isolated_db;
use ed25519_dalek::{Signer, SigningKey};
use rand::rngs::OsRng;
use serde::Deserialize;
use tc_crypto::encode_base64url;
use tc_test_macros::shared_runtime_test;
use tower::ServiceExt;

#[derive(Debug, Deserialize)]
struct BackupResponse {
    encrypted_backup: String,
    root_kid: String,
}

/// Sign up a user and return the app + keys for subsequent requests.
async fn signup_user(username: &str) -> (axum::Router, SignupKeys, common::test_db::IsolatedDb) {
    let db = isolated_db().await;
    let app = TestAppBuilder::new()
        .with_identity_pool(db.pool().clone())
        .build();

    let (json, keys) = valid_signup_with_keys(username);

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/auth/signup")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(json))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::CREATED);

    (app, keys, db)
}

fn backup_request(username: &str) -> Request<Body> {
    Request::builder()
        .method(Method::GET)
        .uri(format!("/auth/backup/{username}"))
        .body(Body::empty())
        .expect("request")
}

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
// GET /auth/backup/{username}
// =========================================================================

#[shared_runtime_test]
async fn test_get_backup_success() {
    let (app, _keys, _db) = signup_user("backupget").await;

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/auth/backup/backupget")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let json: serde_json::Value = serde_json::from_slice(&body).expect("json");

    assert!(
        json["encrypted_backup"].is_string(),
        "response must contain encrypted_backup"
    );
    assert!(
        json["root_kid"].is_string(),
        "response must contain root_kid"
    );
}

#[shared_runtime_test]
async fn test_backup_unknown_user_returns_synthetic() {
    let db = isolated_db().await;
    let app = TestAppBuilder::new()
        .with_identity_pool(db.pool().clone())
        .build();

    let response = app
        .oneshot(backup_request("nonexistentuser"))
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let payload: BackupResponse = serde_json::from_slice(&body).expect("json");
    assert!(!payload.encrypted_backup.is_empty());
    assert!(!payload.root_kid.is_empty());
}

#[shared_runtime_test]
async fn test_backup_synthetic_is_deterministic() {
    let db = isolated_db().await;

    // First request
    let app = TestAppBuilder::new()
        .with_identity_pool(db.pool().clone())
        .build();
    let response = app
        .oneshot(backup_request("deterministicuser"))
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    let body1 = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let payload1: BackupResponse = serde_json::from_slice(&body1).expect("json");

    // Second request — same username should produce same response
    let app = TestAppBuilder::new()
        .with_identity_pool(db.pool().clone())
        .build();
    let response = app
        .oneshot(backup_request("deterministicuser"))
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    let body2 = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let payload2: BackupResponse = serde_json::from_slice(&body2).expect("json");

    assert_eq!(payload1.encrypted_backup, payload2.encrypted_backup);
    assert_eq!(payload1.root_kid, payload2.root_kid);
}

#[shared_runtime_test]
async fn test_backup_synthetic_differs_by_username() {
    let db = isolated_db().await;

    let app = TestAppBuilder::new()
        .with_identity_pool(db.pool().clone())
        .build();
    let response = app
        .oneshot(backup_request("usernamealpha"))
        .await
        .expect("response");
    let body1 = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let payload1: BackupResponse = serde_json::from_slice(&body1).expect("json");

    let app = TestAppBuilder::new()
        .with_identity_pool(db.pool().clone())
        .build();
    let response = app
        .oneshot(backup_request("usernamebeta"))
        .await
        .expect("response");
    let body2 = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let payload2: BackupResponse = serde_json::from_slice(&body2).expect("json");

    assert_ne!(payload1.encrypted_backup, payload2.encrypted_backup);
    assert_ne!(payload1.root_kid, payload2.root_kid);
}

#[shared_runtime_test]
async fn test_backup_existing_user_returns_real_backup() {
    let db = isolated_db().await;

    // Create an account via signup and capture the root_kid
    let app = TestAppBuilder::new()
        .with_identity_pool(db.pool().clone())
        .build();
    let (signup_json, keys) = valid_signup_with_keys("backupuser");
    let signup_response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/auth/signup")
                .header("content-type", "application/json")
                .body(Body::from(signup_json))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(signup_response.status(), StatusCode::CREATED);

    let signup_body = to_bytes(signup_response.into_body(), 1024 * 1024)
        .await
        .expect("signup body");
    let signup_payload: serde_json::Value =
        serde_json::from_slice(&signup_body).expect("signup json");
    let signup_root_kid = signup_payload["root_kid"]
        .as_str()
        .expect("signup root_kid");

    // Verify the signup root_kid matches what we'd derive from the root public key
    let expected_root_kid =
        tc_crypto::Kid::derive(&keys.root_signing_key.verifying_key().to_bytes());
    assert_eq!(
        signup_root_kid,
        expected_root_kid.to_string(),
        "signup root_kid should match derived KID from root pubkey"
    );

    // Fetch the backup
    let app = TestAppBuilder::new()
        .with_identity_pool(db.pool().clone())
        .build();
    let response = app
        .oneshot(backup_request("backupuser"))
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let payload: BackupResponse = serde_json::from_slice(&body).expect("json");
    assert!(!payload.encrypted_backup.is_empty());

    // Cross-reference: the backup endpoint must return the same root_kid from signup,
    // confirming this is the real backup (not a synthetic one).
    assert_eq!(
        payload.root_kid, signup_root_kid,
        "backup root_kid must match signup root_kid — real data, not synthetic"
    );
}

#[shared_runtime_test]
async fn test_backup_existing_user_no_backup_returns_synthetic() {
    let db = isolated_db().await;

    // Sign up a user (creates account + backup)
    let app = TestAppBuilder::new()
        .with_identity_pool(db.pool().clone())
        .build();
    let (signup_json, keys) = valid_signup_with_keys("nobackupuser");
    let signup_response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/auth/signup")
                .header("content-type", "application/json")
                .body(Body::from(signup_json))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(signup_response.status(), StatusCode::CREATED);

    let signup_body = to_bytes(signup_response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let signup_payload: serde_json::Value =
        serde_json::from_slice(&signup_body).expect("signup json");
    let signup_root_kid = signup_payload["root_kid"]
        .as_str()
        .expect("signup root_kid");

    // Delete the backup via the repo so the account exists without one
    let repo = tinycongress_api::identity::repo::PgIdentityRepo::new(db.pool().clone());
    let root_kid = tc_crypto::Kid::derive(&keys.root_signing_key.verifying_key().to_bytes());
    tinycongress_api::identity::repo::IdentityRepo::delete_backup_by_kid(&repo, &root_kid)
        .await
        .expect("delete backup");

    // Fetch the backup — should get a synthetic response
    let app = TestAppBuilder::new()
        .with_identity_pool(db.pool().clone())
        .build();
    let response = app
        .oneshot(backup_request("nobackupuser"))
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let payload: BackupResponse = serde_json::from_slice(&body).expect("json");
    assert!(!payload.encrypted_backup.is_empty());

    // The root_kid should NOT match the signup root_kid because this is synthetic
    assert_ne!(
        payload.root_kid, signup_root_kid,
        "backup root_kid must differ from signup root_kid — synthetic, not real"
    );
}

// =========================================================================
// Success path
// =========================================================================

#[shared_runtime_test]
async fn test_login_success() {
    let (app, keys, _db) = signup_user("loginuser").await;

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
    assert_eq!(response.status(), StatusCode::CREATED);

    let body_bytes = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).expect("json");
    assert!(json["account_id"].is_string());
    assert!(json["root_kid"].is_string());
    assert_eq!(
        json["device_kid"].as_str().unwrap(),
        tc_crypto::Kid::derive(&new_device_pubkey).to_string(),
        "device_kid should be derived from the submitted pubkey"
    );
}

// =========================================================================
// Timestamp validation
// =========================================================================

#[shared_runtime_test]
async fn test_login_expired_timestamp() {
    let (app, keys, _db) = signup_user("expiredts").await;

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
    let (app, keys, _db) = signup_user("futurets").await;

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
    let (app, keys, _db) = signup_user("replaylogin").await;

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
    assert_eq!(response.status(), StatusCode::CREATED);

    // Second request with exact same body is a replay
    let response = app.oneshot(login_request(&body)).await.expect("response");

    // The nonce check fires before create_device_key, so a replayed request
    // must deterministically return BAD_REQUEST for the duplicate nonce.
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body_bytes = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let body_str = String::from_utf8(body_bytes.to_vec()).expect("utf8");
    assert!(
        body_str.contains("replay"),
        "Expected replay error, got: {body_str}"
    );
}

// =========================================================================
// Duplicate device
// =========================================================================

#[shared_runtime_test]
async fn test_login_duplicate_device_returns_conflict() {
    let (app, keys, _db) = signup_user("duplogin").await;

    // Use the SAME device key for two logins, but different timestamps
    let device_key = SigningKey::generate(&mut OsRng);
    let device_pubkey = device_key.verifying_key().to_bytes();

    let timestamp1 = chrono::Utc::now().timestamp();
    let body1 = login_json(
        "duplogin",
        &keys.root_signing_key,
        &device_pubkey,
        "Device A",
        timestamp1,
    );

    // First login succeeds
    let response = app
        .clone()
        .oneshot(login_request(&body1))
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::CREATED);

    // Second login with same pubkey but different timestamp returns 409
    let timestamp2 = timestamp1 + 1;
    let body2 = login_json(
        "duplogin",
        &keys.root_signing_key,
        &device_pubkey,
        "Device A",
        timestamp2,
    );

    let response = app.oneshot(login_request(&body2)).await.expect("response");
    assert_eq!(response.status(), StatusCode::CONFLICT);
}

// =========================================================================
// Certificate verification
// =========================================================================

#[shared_runtime_test]
async fn test_login_old_cert_format_rejected() {
    let (app, keys, _db) = signup_user("oldformat").await;

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
    // Returns 401 with generic message to prevent username enumeration —
    // indistinguishable from AccountNotFound.
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let body_bytes = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let body_str = String::from_utf8(body_bytes.to_vec()).expect("utf8");
    assert!(body_str.contains("Invalid credentials"));
}

#[shared_runtime_test]
async fn test_login_wrong_root_key_rejected() {
    let (app, _keys, _db) = signup_user("wrongroot").await;

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
    // Returns 401 with generic message to prevent username enumeration —
    // indistinguishable from AccountNotFound.
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let body_bytes = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let body_str = String::from_utf8(body_bytes.to_vec()).expect("utf8");
    assert!(body_str.contains("Invalid credentials"));
}

// =========================================================================
// Error paths
// =========================================================================

#[shared_runtime_test]
async fn test_login_handler_account_not_found() {
    let (app, _keys, _db) = signup_user("loginnf").await;

    let root = SigningKey::generate(&mut OsRng);
    let device = SigningKey::generate(&mut OsRng);
    let device_pubkey = device.verifying_key().to_bytes();
    let timestamp = chrono::Utc::now().timestamp();

    let body = login_json("nonexistent", &root, &device_pubkey, "Device", timestamp);

    let response = app.oneshot(login_request(&body)).await.expect("response");
    // Returns 401 (not 400) to prevent username enumeration — same as
    // InvalidCertificate so attackers can't distinguish the two cases.
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let body_bytes = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let body_str = String::from_utf8(body_bytes.to_vec()).expect("utf8");
    assert!(body_str.contains("Invalid credentials"));
}

#[shared_runtime_test]
async fn test_login_empty_username() {
    let db = isolated_db().await;
    let app = TestAppBuilder::new()
        .with_identity_pool(db.pool().clone())
        .build();

    let root = SigningKey::generate(&mut OsRng);
    let device = SigningKey::generate(&mut OsRng);
    let device_pubkey = device.verifying_key().to_bytes();
    let timestamp = chrono::Utc::now().timestamp();

    // Blank username (whitespace only)
    let body = login_json("   ", &root, &device_pubkey, "Device", timestamp);
    let response = app.oneshot(login_request(&body)).await.expect("response");
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body_bytes = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let body_str = String::from_utf8(body_bytes.to_vec()).expect("utf8");
    assert!(
        body_str.contains("Username is required"),
        "Expected username required error, got: {body_str}"
    );
}

#[shared_runtime_test]
async fn test_login_empty_device_name() {
    let db = isolated_db().await;
    let app = TestAppBuilder::new()
        .with_identity_pool(db.pool().clone())
        .build();

    // Sign up a user first
    let (signup_json, keys) = valid_signup_with_keys("emptydevname");
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

    // Login with empty device name
    let new_device_key = SigningKey::generate(&mut OsRng);
    let new_device_pubkey = new_device_key.verifying_key().to_bytes();
    let timestamp = chrono::Utc::now().timestamp();
    let body = login_json(
        "emptydevname",
        &keys.root_signing_key,
        &new_device_pubkey,
        "   ",
        timestamp,
    );

    let response = app.oneshot(login_request(&body)).await.expect("response");
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body_bytes = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let body_str = String::from_utf8(body_bytes.to_vec()).expect("utf8");
    assert!(
        body_str.contains("empty"),
        "Expected empty device name error, got: {body_str}"
    );
}
