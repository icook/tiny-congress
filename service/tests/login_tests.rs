//! Login and backup retrieval integration tests.
//!
//! Tests the unauthenticated login flow:
//! - `GET /auth/backup/{username}` — fetch encrypted backup (returns 200 for both
//!   existing and non-existent users to prevent username enumeration)
//! - `POST /auth/login` — authorize a new device via root-key certificate

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

<<<<<<< HEAD
fn backup_request(username: &str) -> Request<Body> {
    Request::builder()
        .method(Method::GET)
        .uri(format!("/auth/backup/{username}"))
        .body(Body::empty())
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
        .expect("signup body");
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
// POST /auth/login
// =========================================================================

fn login_json(username: &str, keys: &SignupKeys) -> String {
    // Generate a new device key for login (different from signup device)
    let new_device_key = SigningKey::generate(&mut OsRng);
    let new_device_pubkey = new_device_key.verifying_key().to_bytes();
    let cert = keys.root_signing_key.sign(&new_device_pubkey);

    serde_json::json!({
        "username": username,
        "device": {
            "pubkey": encode_base64url(&new_device_pubkey),
            "name": "Login Device",
            "certificate": encode_base64url(&cert.to_bytes()),
        }
    })
    .to_string()
}

#[shared_runtime_test]
async fn test_login_handler_success() {
    let (app, keys, _db) = signup_user("logintest").await;

    let body = login_json("logintest", &keys);

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/auth/login")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(body))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::CREATED);

    let resp_body = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let body_str = String::from_utf8(resp_body.to_vec()).expect("utf8");
    assert!(body_str.contains("account_id"));
    assert!(body_str.contains("root_kid"));
    assert!(body_str.contains("device_kid"));
}

#[shared_runtime_test]
async fn test_login_handler_account_not_found() {
    let (app, keys, _db) = signup_user("loginnf").await;

    let body = login_json("nonexistent", &keys);

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/auth/login")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(body))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[shared_runtime_test]
async fn test_login_handler_invalid_certificate() {
    let (app, _keys, _db) = signup_user("loginbadcert").await;

    // Generate a device key but sign with a random key (not the root)
    let new_device_key = SigningKey::generate(&mut OsRng);
    let new_device_pubkey = new_device_key.verifying_key().to_bytes();
    let wrong_root = SigningKey::generate(&mut OsRng);
    let bad_cert = wrong_root.sign(&new_device_pubkey);

    let body = serde_json::json!({
        "username": "loginbadcert",
        "device": {
            "pubkey": encode_base64url(&new_device_pubkey),
            "name": "Bad Cert Device",
            "certificate": encode_base64url(&bad_cert.to_bytes()),
        }
    })
    .to_string();

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/auth/login")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(body))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[shared_runtime_test]
async fn test_login_handler_duplicate_device() {
    let (app, keys, _db) = signup_user("logindup").await;

    // Generate a device key and login with it
    let new_device_key = SigningKey::generate(&mut OsRng);
    let new_device_pubkey = new_device_key.verifying_key().to_bytes();
    let cert = keys.root_signing_key.sign(&new_device_pubkey);

    let body = serde_json::json!({
        "username": "logindup",
        "device": {
            "pubkey": encode_base64url(&new_device_pubkey),
            "name": "First Login",
            "certificate": encode_base64url(&cert.to_bytes()),
        }
    })
    .to_string();

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/auth/login")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(body))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::CREATED);

    // Same device key again should conflict
    let body2 = serde_json::json!({
        "username": "logindup",
        "device": {
            "pubkey": encode_base64url(&new_device_pubkey),
            "name": "Second Login",
            "certificate": encode_base64url(&cert.to_bytes()),
        }
    })
    .to_string();

    let response2 = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/auth/login")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(body2))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response2.status(), StatusCode::CONFLICT);
}

#[shared_runtime_test]
async fn test_login_handler_empty_username() {
    let db = isolated_db().await;
    let app = TestAppBuilder::new()
        .with_identity_pool(db.pool().clone())
        .build();

    let body = serde_json::json!({
        "username": "   ",
        "device": {
            "pubkey": encode_base64url(&[1u8; 32]),
            "name": "Test Device",
            "certificate": encode_base64url(&[0u8; 64]),
        }
    })
    .to_string();

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/auth/login")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(body))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}
