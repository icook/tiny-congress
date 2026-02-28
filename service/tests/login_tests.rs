//! Login and backup retrieval integration tests.
//!
//! Tests the unauthenticated login flow:
//! - `GET /auth/backup/{username}` — fetch encrypted backup for a known user
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
use tc_crypto::encode_base64url;
use tc_test_macros::shared_runtime_test;
use tower::ServiceExt;

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
async fn test_get_backup_not_found() {
    let db = isolated_db().await;
    let app = TestAppBuilder::new()
        .with_identity_pool(db.pool().clone())
        .build();

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/auth/backup/nonexistent")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

// =========================================================================
// POST /auth/login
// =========================================================================

#[shared_runtime_test]
async fn test_login_success() {
    let (app, keys, _db) = signup_user("loginuser").await;

    // Generate a NEW device keypair (distinct from the signup device)
    let new_device_key = SigningKey::generate(&mut OsRng);
    let new_device_pubkey = new_device_key.verifying_key().to_bytes();

    // Sign the new device pubkey with the root key to produce a certificate
    let cert = keys.root_signing_key.sign(&new_device_pubkey);

    let body = serde_json::json!({
        "username": "loginuser",
        "device": {
            "pubkey": encode_base64url(&new_device_pubkey),
            "name": "Login Device",
            "certificate": encode_base64url(&cert.to_bytes()),
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

    assert_eq!(response.status(), StatusCode::CREATED);

    let resp_body = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let json: serde_json::Value = serde_json::from_slice(&resp_body).expect("json");

    assert!(
        json["account_id"].is_string(),
        "response must contain account_id"
    );
    assert!(
        json["root_kid"].is_string(),
        "response must contain root_kid"
    );
    assert!(
        json["device_kid"].is_string(),
        "response must contain device_kid"
    );
}

#[shared_runtime_test]
async fn test_login_unknown_username() {
    let db = isolated_db().await;
    let app = TestAppBuilder::new()
        .with_identity_pool(db.pool().clone())
        .build();

    // Generate a throwaway device keypair and root keypair
    let root_key = SigningKey::generate(&mut OsRng);
    let device_key = SigningKey::generate(&mut OsRng);
    let device_pubkey = device_key.verifying_key().to_bytes();
    let cert = root_key.sign(&device_pubkey);

    let body = serde_json::json!({
        "username": "nobody_here",
        "device": {
            "pubkey": encode_base64url(&device_pubkey),
            "name": "Ghost Device",
            "certificate": encode_base64url(&cert.to_bytes()),
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

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[shared_runtime_test]
async fn test_login_invalid_certificate() {
    let (app, _keys, _db) = signup_user("badcertlogin").await;

    // Generate a new device keypair
    let new_device_key = SigningKey::generate(&mut OsRng);
    let new_device_pubkey = new_device_key.verifying_key().to_bytes();

    // Sign with a WRONG root key (not the one used during signup)
    let wrong_root_key = SigningKey::generate(&mut OsRng);
    let bad_cert = wrong_root_key.sign(&new_device_pubkey);

    let body = serde_json::json!({
        "username": "badcertlogin",
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

    let resp_body = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let json: serde_json::Value = serde_json::from_slice(&resp_body).expect("json");
    assert!(
        json["error"]
            .as_str()
            .unwrap_or("")
            .contains("Invalid device certificate"),
        "error should mention invalid certificate"
    );
}

#[shared_runtime_test]
async fn test_login_empty_username() {
    let db = isolated_db().await;
    let app = TestAppBuilder::new()
        .with_identity_pool(db.pool().clone())
        .build();

    let device_key = SigningKey::generate(&mut OsRng);
    let device_pubkey = device_key.verifying_key().to_bytes();
    let root_key = SigningKey::generate(&mut OsRng);
    let cert = root_key.sign(&device_pubkey);

    let body = serde_json::json!({
        "username": "   ",
        "device": {
            "pubkey": encode_base64url(&device_pubkey),
            "name": "My Device",
            "certificate": encode_base64url(&cert.to_bytes()),
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
async fn test_login_empty_device_name() {
    let (app, keys, _db) = signup_user("emptyname").await;

    let new_device_key = SigningKey::generate(&mut OsRng);
    let new_device_pubkey = new_device_key.verifying_key().to_bytes();
    let cert = keys.root_signing_key.sign(&new_device_pubkey);

    let body = serde_json::json!({
        "username": "emptyname",
        "device": {
            "pubkey": encode_base64url(&new_device_pubkey),
            "name": "   ",
            "certificate": encode_base64url(&cert.to_bytes()),
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
async fn test_login_duplicate_device_returns_conflict() {
    let (app, keys, _db) = signup_user("duplogin").await;

    // Use the SAME device key for two logins
    let device_key = SigningKey::generate(&mut OsRng);
    let device_pubkey = device_key.verifying_key().to_bytes();
    let cert = keys.root_signing_key.sign(&device_pubkey);

    let body = serde_json::json!({
        "username": "duplogin",
        "device": {
            "pubkey": encode_base64url(&device_pubkey),
            "name": "Device A",
            "certificate": encode_base64url(&cert.to_bytes()),
        }
    })
    .to_string();

    // First login succeeds
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/auth/login")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(body.clone()))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::CREATED);

    // Second login with same pubkey returns 409
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
    assert_eq!(response.status(), StatusCode::CONFLICT);
}

#[shared_runtime_test]
async fn test_login_creates_device_with_correct_kid() {
    let (app, keys, _db) = signup_user("loginstate").await;

    // Login creates a new device
    let new_device_key = SigningKey::generate(&mut OsRng);
    let new_device_pubkey = new_device_key.verifying_key().to_bytes();
    let cert = keys.root_signing_key.sign(&new_device_pubkey);

    let expected_kid = tc_crypto::Kid::derive(&new_device_pubkey);

    let body = serde_json::json!({
        "username": "loginstate",
        "device": {
            "pubkey": encode_base64url(&new_device_pubkey),
            "name": "New Login Device",
            "certificate": encode_base64url(&cert.to_bytes()),
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
    assert_eq!(response.status(), StatusCode::CREATED);

    let resp_body = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let login_json: serde_json::Value = serde_json::from_slice(&resp_body).expect("json");

    // Verify device_kid matches what we'd derive from the pubkey
    assert_eq!(
        login_json["device_kid"].as_str().expect("device_kid"),
        expected_kid.to_string(),
        "device_kid should be derived from the submitted pubkey"
    );
}
