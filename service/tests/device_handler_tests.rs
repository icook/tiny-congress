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
    let body_hash = Sha256::digest(body);
    let body_hash_hex = format!("{body_hash:x}");
    let canonical = format!("{method}\n{path}\n{timestamp}\n{body_hash_hex}");
    let signature = signing_key.sign(canonical.as_bytes());

    vec![
        ("X-Device-Kid", kid.to_string()),
        ("X-Signature", encode_base64url(&signature.to_bytes())),
        ("X-Timestamp", timestamp.to_string()),
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
    let body_hash = Sha256::digest(b"");
    let body_hash_hex = format!("{body_hash:x}");
    let canonical = format!("GET\n/auth/devices\n{old_timestamp}\n{body_hash_hex}");
    let signature = keys.device_signing_key.sign(canonical.as_bytes());

    let req = Request::builder()
        .method(Method::GET)
        .uri("/auth/devices")
        .header("X-Device-Kid", keys.device_kid.to_string())
        .header("X-Signature", encode_base64url(&signature.to_bytes()))
        .header("X-Timestamp", old_timestamp.to_string())
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

#[shared_runtime_test]
async fn test_add_device_invalid_certificate() {
    let (app, keys, _db) = signup_user("badcert").await;

    // Generate a new device key but sign its pubkey with a random key (not the root)
    let new_device_key = SigningKey::generate(&mut OsRng);
    let new_device_pubkey = new_device_key.verifying_key().to_bytes();
    let wrong_root = SigningKey::generate(&mut OsRng);
    let bad_cert = wrong_root.sign(&new_device_pubkey);

    let body = serde_json::json!({
        "pubkey": encode_base64url(&new_device_pubkey),
        "name": "Bad Cert Device",
        "certificate": encode_base64url(&bad_cert.to_bytes()),
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
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[shared_runtime_test]
async fn test_add_device_duplicate_returns_conflict() {
    let (app, keys, _db) = signup_user("dupdev").await;

    // Generate a new device and add it
    let new_device_key = SigningKey::generate(&mut OsRng);
    let new_device_pubkey = new_device_key.verifying_key().to_bytes();
    let cert = keys.root_signing_key.sign(&new_device_pubkey);

    let body = serde_json::json!({
        "pubkey": encode_base64url(&new_device_pubkey),
        "name": "Duplicate Device",
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

    // Same pubkey again with a different name so the body hash (and thus the
    // signature) differs — otherwise replay protection rejects the request
    // before the repo layer can detect the duplicate KID.
    let body2 = serde_json::json!({
        "pubkey": encode_base64url(&new_device_pubkey),
        "name": "Duplicate Device 2",
        "certificate": encode_base64url(&cert.to_bytes()),
    })
    .to_string();

    let req2 = build_authed_request(
        Method::POST,
        "/auth/devices",
        &body2,
        &keys.device_signing_key,
        &keys.device_kid,
    );
    let response2 = app.oneshot(req2).await.expect("response");
    assert_eq!(response2.status(), StatusCode::CONFLICT);
}

// =========================================================================
// DELETE /auth/devices/:kid
// =========================================================================

#[shared_runtime_test]
async fn test_revoke_device_success() {
    let (app, keys, _db) = signup_user("revokedev").await;

    // First, add a second device
    let new_device_key = SigningKey::generate(&mut OsRng);
    let new_device_pubkey = new_device_key.verifying_key().to_bytes();
    let cert = keys.root_signing_key.sign(&new_device_pubkey);
    let new_device_kid = Kid::derive(&new_device_pubkey);

    let add_body = serde_json::json!({
        "pubkey": encode_base64url(&new_device_pubkey),
        "name": "To Revoke",
        "certificate": encode_base64url(&cert.to_bytes()),
    })
    .to_string();

    let add_req = build_authed_request(
        Method::POST,
        "/auth/devices",
        &add_body,
        &keys.device_signing_key,
        &keys.device_kid,
    );

    let add_response = app.clone().oneshot(add_req).await.expect("response");
    assert_eq!(add_response.status(), StatusCode::CREATED);

    // Now revoke it
    let revoke_path = format!("/auth/devices/{new_device_kid}");
    let revoke_req = build_authed_request(
        Method::DELETE,
        &revoke_path,
        "",
        &keys.device_signing_key,
        &keys.device_kid,
    );

    let revoke_response = app.oneshot(revoke_req).await.expect("response");
    assert_eq!(revoke_response.status(), StatusCode::NO_CONTENT);
}

#[shared_runtime_test]
async fn test_revoke_self_fails() {
    let (app, keys, _db) = signup_user("revokeself").await;

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

    let response = app.oneshot(req).await.expect("response");
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
}

#[shared_runtime_test]
async fn test_rename_device_empty_name_fails() {
    let (app, keys, _db) = signup_user("renamebad").await;

    let path = format!("/auth/devices/{}", keys.device_kid);
    let body = serde_json::json!({ "name": "   " }).to_string();

    let req = build_authed_request(
        Method::PATCH,
        &path,
        &body,
        &keys.device_signing_key,
        &keys.device_kid,
    );

    let response = app.oneshot(req).await.expect("response");
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

// =========================================================================
// Replay protection
// =========================================================================

#[shared_runtime_test]
async fn test_replay_is_blocked() {
    let (app, keys, _db) = signup_user("replaytest").await;

    // Build headers once — both requests will share the exact same signature
    let headers = sign_request(
        "GET",
        "/auth/devices",
        b"",
        &keys.device_signing_key,
        &keys.device_kid,
    );

    let build_req = |hdrs: &[(&'static str, String)]| {
        let mut builder = Request::builder().method(Method::GET).uri("/auth/devices");
        for (name, value) in hdrs {
            builder = builder.header(*name, value);
        }
        builder.body(Body::empty()).expect("request")
    };

    // First request succeeds
    let response = app
        .clone()
        .oneshot(build_req(&headers))
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);

    // Exact same signed request is rejected as a replay
    let response = app.oneshot(build_req(&headers)).await.expect("response");
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

// =========================================================================
// Cross-account authorization
// =========================================================================

/// Sign up a second user into an existing DB pool.
async fn signup_user_in_pool(username: &str, pool: &sqlx::PgPool) -> (axum::Router, SignupKeys) {
    let app = TestAppBuilder::new()
        .with_identity_pool(pool.clone())
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

    (app, keys)
}

#[shared_runtime_test]
async fn test_cannot_revoke_other_accounts_device() {
    let (app, _keys_a, db) = signup_user("ownerA").await;
    let (_app_b, keys_b) = signup_user_in_pool("ownerB", db.pool()).await;

    // Account A tries to revoke account B's device
    let path = format!("/auth/devices/{}", keys_b.device_kid);
    let req = build_authed_request(
        Method::DELETE,
        &path,
        "",
        &_keys_a.device_signing_key,
        &_keys_a.device_kid,
    );

    let response = app.oneshot(req).await.expect("response");
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[shared_runtime_test]
async fn test_cannot_rename_other_accounts_device() {
    let (app, _keys_a, db) = signup_user("renameOwnerA").await;
    let (_app_b, keys_b) = signup_user_in_pool("renameOwnerB", db.pool()).await;

    // Account A tries to rename account B's device
    let path = format!("/auth/devices/{}", keys_b.device_kid);
    let body = serde_json::json!({ "name": "Hijacked" }).to_string();
    let req = build_authed_request(
        Method::PATCH,
        &path,
        &body,
        &_keys_a.device_signing_key,
        &_keys_a.device_kid,
    );

    let response = app.oneshot(req).await.expect("response");
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
