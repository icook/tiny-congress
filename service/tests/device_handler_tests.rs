//! Device management handler integration tests.
//!
//! Tests the authenticated device endpoints (GET/POST/DELETE/PATCH /auth/devices)
//! with real database connections.

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
use sha2::{Digest, Sha256};
use tc_crypto::{encode_base64url, Kid};
use tc_test_macros::shared_runtime_test;
use tower::ServiceExt;

/// Build authenticated request headers for device endpoints.
fn sign_request(
    method: &str,
    path: &str,
    body: &[u8],
    signing_key: &SigningKey,
    kid: &Kid,
) -> Vec<(&'static str, String)> {
    let timestamp = chrono::Utc::now().timestamp();
    let nonce = uuid::Uuid::new_v4().to_string();
    let body_hash = Sha256::digest(body);
    let body_hash_hex = format!("{body_hash:x}");
    let canonical = format!("{method}\n{path}\n{timestamp}\n{nonce}\n{body_hash_hex}");
    let signature = signing_key.sign(canonical.as_bytes());

    vec![
        ("X-Device-Kid", kid.to_string()),
        ("X-Signature", encode_base64url(&signature.to_bytes())),
        ("X-Timestamp", timestamp.to_string()),
        ("X-Nonce", nonce),
    ]
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

fn build_authed_request(
    method: Method,
    path: &str,
    body: &str,
    signing_key: &SigningKey,
    kid: &Kid,
) -> Request<Body> {
    let headers = sign_request(method.as_str(), path, body.as_bytes(), signing_key, kid);

    let mut builder = Request::builder().method(method).uri(path);

    for (name, value) in &headers {
        builder = builder.header(*name, value);
    }

    if !body.is_empty() {
        builder = builder.header(CONTENT_TYPE, "application/json");
    }

    builder.body(Body::from(body.to_string())).expect("request")
}

// =========================================================================
// GET /auth/devices
// =========================================================================

#[shared_runtime_test]
async fn test_list_devices_success() {
    let (app, keys, _db) = signup_user("listdev").await;

    let req = build_authed_request(
        Method::GET,
        "/auth/devices",
        "",
        &keys.device_signing_key,
        &keys.device_kid,
    );

    let response = app.oneshot(req).await.expect("response");
    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let json: serde_json::Value = serde_json::from_slice(&body).expect("json");
    let devices = json["devices"].as_array().expect("devices array");
    assert_eq!(devices.len(), 1);
    assert_eq!(devices[0]["device_name"], "Test Device");
}

#[shared_runtime_test]
async fn test_list_devices_no_auth() {
    let (_app, _keys, db) = signup_user("noauthlist").await;
    let app = TestAppBuilder::new()
        .with_identity_pool(db.pool().clone())
        .build();

    let req = Request::builder()
        .method(Method::GET)
        .uri("/auth/devices")
        .body(Body::empty())
        .expect("request");

    let response = app.oneshot(req).await.expect("response");
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[shared_runtime_test]
async fn test_list_devices_invalid_signature() {
    let (app, keys, _db) = signup_user("badsiglist").await;

    // Sign with a different key
    let wrong_key = SigningKey::generate(&mut OsRng);
    let req = build_authed_request(
        Method::GET,
        "/auth/devices",
        "",
        &wrong_key,
        &keys.device_kid,
    );

    let response = app.oneshot(req).await.expect("response");
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[shared_runtime_test]
async fn test_list_devices_expired_timestamp() {
    let (_app, keys, db) = signup_user("expiredts").await;
    let app = TestAppBuilder::new()
        .with_identity_pool(db.pool().clone())
        .build();

    let old_timestamp = chrono::Utc::now().timestamp() - 600; // 10 min ago
    let nonce = uuid::Uuid::new_v4().to_string();
    let body_hash = Sha256::digest(b"");
    let body_hash_hex = format!("{body_hash:x}");
    let canonical = format!("GET\n/auth/devices\n{old_timestamp}\n{nonce}\n{body_hash_hex}");
    let signature = keys.device_signing_key.sign(canonical.as_bytes());

    let req = Request::builder()
        .method(Method::GET)
        .uri("/auth/devices")
        .header("X-Device-Kid", keys.device_kid.to_string())
        .header("X-Signature", encode_base64url(&signature.to_bytes()))
        .header("X-Timestamp", old_timestamp.to_string())
        .header("X-Nonce", &nonce)
        .body(Body::empty())
        .expect("request");

    let response = app.oneshot(req).await.expect("response");
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

// =========================================================================
// POST /auth/devices
// =========================================================================

#[shared_runtime_test]
async fn test_add_device_success() {
    let (app, keys, _db) = signup_user("adddev").await;

    // Generate a new device key and certificate
    let new_device_key = SigningKey::generate(&mut OsRng);
    let new_device_pubkey = new_device_key.verifying_key().to_bytes();
    let cert = keys.root_signing_key.sign(&new_device_pubkey);

    let body = serde_json::json!({
        "pubkey": encode_base64url(&new_device_pubkey),
        "name": "Second Device",
        "certificate": encode_base64url(&cert.to_bytes()),
    })
    .to_string();

    let req = build_authed_request(
        Method::POST,
        "/auth/devices",
        &body,
        &keys.device_signing_key,
        &keys.device_kid,
    );

    let response = app.oneshot(req).await.expect("response");
    assert_eq!(response.status(), StatusCode::CREATED);

    let resp_body = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let json: serde_json::Value = serde_json::from_slice(&resp_body).expect("json");
    assert!(json["device_kid"].is_string());
    assert!(json["created_at"].is_string());
}

// =========================================================================
// DELETE /auth/devices/:kid
// =========================================================================

#[shared_runtime_test]
async fn test_revoke_device_success() {
    let (app, keys, _db) = signup_user("revokedev").await;

    // First add a second device
    let new_device_key = SigningKey::generate(&mut OsRng);
    let new_device_pubkey = new_device_key.verifying_key().to_bytes();
    let cert = keys.root_signing_key.sign(&new_device_pubkey);
    let new_device_kid = Kid::derive(&new_device_pubkey);

    let body = serde_json::json!({
        "pubkey": encode_base64url(&new_device_pubkey),
        "name": "To Revoke",
        "certificate": encode_base64url(&cert.to_bytes()),
    })
    .to_string();

    let req = build_authed_request(
        Method::POST,
        "/auth/devices",
        &body,
        &keys.device_signing_key,
        &keys.device_kid,
    );
    let response = app.clone().oneshot(req).await.expect("response");
    assert_eq!(response.status(), StatusCode::CREATED);

    // Now revoke it
    let path = format!("/auth/devices/{new_device_kid}");
    let req = build_authed_request(
        Method::DELETE,
        &path,
        "",
        &keys.device_signing_key,
        &keys.device_kid,
    );
    let response = app.oneshot(req).await.expect("response");
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
}

#[shared_runtime_test]
async fn test_revoke_device_wrong_account() {
    // Both users must be in the same database so the cross-account
    // ownership check (`device.account_id != account_id`) actually fires.
    let db = isolated_db().await;
    let app = TestAppBuilder::new()
        .with_identity_pool(db.pool().clone())
        .build();

    // Sign up user 1
    let (json1, keys1) = valid_signup_with_keys("revwrong1");
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/auth/signup")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(json1))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::CREATED);

    // Sign up user 2 in the same database
    let (json2, keys2) = valid_signup_with_keys("revwrong2");
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/auth/signup")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(json2))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::CREATED);

    // User 1 tries to revoke user 2's device — should get 404 from ownership check
    let path = format!("/auth/devices/{}", keys2.device_kid);
    let req = build_authed_request(
        Method::DELETE,
        &path,
        "",
        &keys1.device_signing_key,
        &keys1.device_kid,
    );
    let response = app.oneshot(req).await.expect("response");
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[shared_runtime_test]
async fn test_revoke_device_self_revocation_rejected() {
    let (app, keys, _db) = signup_user("selfrevoke").await;

    // Try to revoke own device — should get 422
    let path = format!("/auth/devices/{}", keys.device_kid);
    let req = build_authed_request(
        Method::DELETE,
        &path,
        "",
        &keys.device_signing_key,
        &keys.device_kid,
    );
    let response = app.oneshot(req).await.expect("response");
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

// =========================================================================
// PATCH /auth/devices/:kid
// =========================================================================

#[shared_runtime_test]
async fn test_rename_device_success() {
    let (app, keys, _db) = signup_user("renamedev").await;

    let path = format!("/auth/devices/{}", keys.device_kid);
    let body = serde_json::json!({ "name": "Renamed Device" }).to_string();

    let req = build_authed_request(
        Method::PATCH,
        &path,
        &body,
        &keys.device_signing_key,
        &keys.device_kid,
    );

    let response = app.clone().oneshot(req).await.expect("response");
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    // Verify it was renamed by listing
    let req = build_authed_request(
        Method::GET,
        "/auth/devices",
        "",
        &keys.device_signing_key,
        &keys.device_kid,
    );
    let response = app.oneshot(req).await.expect("response");
    let body = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let json: serde_json::Value = serde_json::from_slice(&body).expect("json");
    assert_eq!(json["devices"][0]["device_name"], "Renamed Device");
}

// =========================================================================
// Nonce replay prevention
// =========================================================================

#[shared_runtime_test]
async fn test_nonce_replay_rejected() {
    let (app, keys, _db) = signup_user("noncereplay").await;

    // Build a request with a specific nonce
    let nonce = "fixed-nonce-for-replay-test";
    let timestamp = chrono::Utc::now().timestamp();
    let body_hash = Sha256::digest(b"");
    let body_hash_hex = format!("{body_hash:x}");
    let canonical = format!("GET\n/auth/devices\n{timestamp}\n{nonce}\n{body_hash_hex}");
    let signature = keys.device_signing_key.sign(canonical.as_bytes());

    let build_req = || {
        Request::builder()
            .method(Method::GET)
            .uri("/auth/devices")
            .header("X-Device-Kid", keys.device_kid.to_string())
            .header("X-Signature", encode_base64url(&signature.to_bytes()))
            .header("X-Timestamp", timestamp.to_string())
            .header("X-Nonce", nonce)
            .body(Body::empty())
            .expect("request")
    };

    // First request succeeds
    let response = app.clone().oneshot(build_req()).await.expect("response");
    assert_eq!(response.status(), StatusCode::OK);

    // Same nonce replayed — should be rejected
    let response = app.oneshot(build_req()).await.expect("response");
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

// =========================================================================
// Auth with revoked device
// =========================================================================

#[shared_runtime_test]
async fn test_auth_with_revoked_device() {
    let (app, keys, _db) = signup_user("revokedauth").await;

    // Add a second device
    let new_device_key = SigningKey::generate(&mut OsRng);
    let new_device_pubkey = new_device_key.verifying_key().to_bytes();
    let cert = keys.root_signing_key.sign(&new_device_pubkey);
    let new_device_kid = Kid::derive(&new_device_pubkey);

    let body = serde_json::json!({
        "pubkey": encode_base64url(&new_device_pubkey),
        "name": "Soon Revoked",
        "certificate": encode_base64url(&cert.to_bytes()),
    })
    .to_string();

    let req = build_authed_request(
        Method::POST,
        "/auth/devices",
        &body,
        &keys.device_signing_key,
        &keys.device_kid,
    );
    let response = app.clone().oneshot(req).await.expect("response");
    assert_eq!(response.status(), StatusCode::CREATED);

    // Revoke the second device
    let path = format!("/auth/devices/{new_device_kid}");
    let req = build_authed_request(
        Method::DELETE,
        &path,
        "",
        &keys.device_signing_key,
        &keys.device_kid,
    );
    let response = app.clone().oneshot(req).await.expect("response");
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    // Try to authenticate with the revoked device — should get 403
    let req = build_authed_request(
        Method::GET,
        "/auth/devices",
        "",
        &new_device_key,
        &new_device_kid,
    );
    let response = app.oneshot(req).await.expect("response");
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}
