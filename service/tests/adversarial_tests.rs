//! Adversarial integration tests targeting the cryptographic trust boundary.
//!
//! These tests probe the boundary between client-side crypto operations and
//! server-side signature/certificate verification. Each test represents an
//! attack vector where the server must correctly detect and reject malicious input.
//!
//! Focus area: trust-boundary
//!
//! Run with:
//! ```
//! cargo test --test adversarial_tests -- --test-threads=1
//! ```

mod common;

use axum::{
    body::Body,
    http::{header::CONTENT_TYPE, Method, Request, StatusCode},
};
use common::app_builder::TestAppBuilder;
use common::factories::{valid_signup_with_keys, SignupKeys};
use common::test_db::isolated_db;
use ed25519_dalek::{Signer, SigningKey};
use rand::rngs::OsRng;
use sha2::{Digest, Sha256};
use tc_crypto::{encode_base64url, BackupEnvelope, Kid};
use tc_test_macros::shared_runtime_test;
use tower::ServiceExt;

// =========================================================================
// Shared helpers (mirrors patterns from device_handler_tests.rs)
// =========================================================================

/// Build the auth headers for a device-authenticated request.
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

/// Build an authenticated request for a device endpoint.
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

/// Sign up a new user and return the app, signing keys, and isolated DB.
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

/// Sign up a second user into an existing DB pool (for cross-account tests).
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

/// Build a valid backup envelope blob (base64url-encoded).
fn valid_backup_blob() -> String {
    let envelope = BackupEnvelope::build([0xAA; 16], 65536, 3, 1, [0xBB; 12], &[0xCC; 48])
        .expect("test envelope");
    encode_base64url(envelope.as_bytes())
}

// =========================================================================
// 1. Trust Boundary: Certificate Forgery at Signup
// =========================================================================

/// Attack: Submit a signup request where the device certificate is signed by a
/// random key rather than the account's root key.
///
/// Expected: 400 Bad Request — the server must verify that the device certificate
/// was signed by the root key declared in the signup request. If this test fails,
/// an attacker could sign up with device keys they control independently of any
/// root key, breaking the delegation chain where authority flows root → device.
#[shared_runtime_test]
async fn test_signup_forged_certificate_rejected() {
    let db = isolated_db().await;
    let app = TestAppBuilder::new()
        .with_identity_pool(db.pool().clone())
        .build();

    let root_signing_key = SigningKey::generate(&mut OsRng);
    let root_pubkey = encode_base64url(&root_signing_key.verifying_key().to_bytes());

    let device_signing_key = SigningKey::generate(&mut OsRng);
    let device_pubkey_bytes = device_signing_key.verifying_key().to_bytes();
    let device_pubkey = encode_base64url(&device_pubkey_bytes);

    // Sign device pubkey with an attacker's random key — NOT the account's root key.
    let attacker_key = SigningKey::generate(&mut OsRng);
    let forged_cert = attacker_key.sign(&device_pubkey_bytes);
    let certificate = encode_base64url(&forged_cert.to_bytes());

    let backup_blob = valid_backup_blob();
    let json = format!(
        r#"{{"username":"adv_forged_cert","root_pubkey":"{root_pubkey}","backup":{{"encrypted_blob":"{backup_blob}"}},"device":{{"pubkey":"{device_pubkey}","name":"Test Device","certificate":"{certificate}"}}}}"#
    );

    let response = app
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

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

/// Attack: Sign the device certificate with Account B's root key but submit it
/// as Account A's signup — a cross-account certificate injection at signup.
///
/// Expected: 400 Bad Request — the certificate must verify against the root_pubkey
/// declared in the request (Account A's). If this test fails, an attacker who
/// controls one legitimate account could sign up under any username by generating
/// device certificates with their own root key, then presenting a different
/// root_pubkey, effectively breaking the root key ↔ device delegation model.
#[shared_runtime_test]
async fn test_signup_cross_account_certificate_rejected() {
    let db = isolated_db().await;
    let app = TestAppBuilder::new()
        .with_identity_pool(db.pool().clone())
        .build();

    // Account A's root key — this is what will appear in the signup request.
    let root_a = SigningKey::generate(&mut OsRng);
    let root_a_pubkey = encode_base64url(&root_a.verifying_key().to_bytes());

    // Account B's root key — the attacker controls this separately.
    let root_b = SigningKey::generate(&mut OsRng);

    // Device key to register under Account A.
    let device_key = SigningKey::generate(&mut OsRng);
    let device_pubkey_bytes = device_key.verifying_key().to_bytes();
    let device_pubkey = encode_base64url(&device_pubkey_bytes);

    // Certificate signed by Account B's root key, not Account A's.
    let cross_cert = root_b.sign(&device_pubkey_bytes);
    let certificate = encode_base64url(&cross_cert.to_bytes());

    let backup_blob = valid_backup_blob();
    let json = format!(
        r#"{{"username":"adv_cross_cert","root_pubkey":"{root_a_pubkey}","backup":{{"encrypted_blob":"{backup_blob}"}},"device":{{"pubkey":"{device_pubkey}","name":"Test Device","certificate":"{certificate}"}}}}"#
    );

    let response = app
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

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

/// Attack: Flip one bit in an otherwise-valid signup device certificate.
///
/// Expected: 400 Bad Request — Ed25519 signature verification is deterministic;
/// a single bit flip in a 64-byte signature must produce a verification failure.
/// If this test fails, the server is not performing proper cryptographic
/// verification, which would allow trivially mutated certificates to be accepted
/// (a fundamental break in the trust model).
#[shared_runtime_test]
async fn test_signup_bit_flipped_certificate_rejected() {
    let db = isolated_db().await;
    let app = TestAppBuilder::new()
        .with_identity_pool(db.pool().clone())
        .build();

    let root_signing_key = SigningKey::generate(&mut OsRng);
    let root_pubkey = encode_base64url(&root_signing_key.verifying_key().to_bytes());

    let device_signing_key = SigningKey::generate(&mut OsRng);
    let device_pubkey_bytes = device_signing_key.verifying_key().to_bytes();
    let device_pubkey = encode_base64url(&device_pubkey_bytes);

    // Start with a valid certificate, then flip one bit in the middle.
    let valid_sig = root_signing_key.sign(&device_pubkey_bytes);
    let mut cert_bytes = valid_sig.to_bytes();
    cert_bytes[32] ^= 0x01;
    let certificate = encode_base64url(&cert_bytes);

    let backup_blob = valid_backup_blob();
    let json = format!(
        r#"{{"username":"adv_bit_flip","root_pubkey":"{root_pubkey}","backup":{{"encrypted_blob":"{backup_blob}"}},"device":{{"pubkey":"{device_pubkey}","name":"Test Device","certificate":"{certificate}"}}}}"#
    );

    let response = app
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

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

// =========================================================================
// 2. Trust Boundary: Revoked Device Auth
// =========================================================================

/// Attack: Revoke a device, then attempt to authenticate with it.
///
/// Expected: 403 Forbidden — the server verifies the signature first (the private
/// key still produces cryptographically valid signatures), but must then reject the
/// request because the device has been revoked. The 403 (not 401) confirms that
/// the signature passed verification before the revocation check fired. If this
/// test fails, a compromised device key can authenticate indefinitely after an
/// account owner revokes it, leaving accounts permanently exposed after a
/// key-compromise event.
#[shared_runtime_test]
async fn test_revoked_device_auth_rejected() {
    let (app, keys, _db) = signup_user("adv_revoke_auth").await;

    // Add device B — this is the one we will revoke.
    let device_b_key = SigningKey::generate(&mut OsRng);
    let device_b_pubkey_bytes = device_b_key.verifying_key().to_bytes();
    let device_b_kid = Kid::derive(&device_b_pubkey_bytes);
    let cert_b = keys.root_signing_key.sign(&device_b_pubkey_bytes);

    let add_body = serde_json::json!({
        "pubkey": encode_base64url(&device_b_pubkey_bytes),
        "name": "Device B",
        "certificate": encode_base64url(&cert_b.to_bytes()),
    })
    .to_string();

    let add_req = build_authed_request(
        Method::POST,
        "/auth/devices",
        &add_body,
        &keys.device_signing_key,
        &keys.device_kid,
    );
    let add_resp = app.clone().oneshot(add_req).await.expect("response");
    assert_eq!(add_resp.status(), StatusCode::CREATED);

    // Revoke device B using the original device A.
    let revoke_path = format!("/auth/devices/{device_b_kid}");
    let revoke_req = build_authed_request(
        Method::DELETE,
        &revoke_path,
        "",
        &keys.device_signing_key,
        &keys.device_kid,
    );
    let revoke_resp = app.clone().oneshot(revoke_req).await.expect("response");
    assert_eq!(revoke_resp.status(), StatusCode::NO_CONTENT);

    // Now try to authenticate with the revoked device B.
    // The signature itself is valid — the private key is unchanged — but the
    // device record is revoked. Must get 403, not 401.
    let req = build_authed_request(
        Method::GET,
        "/auth/devices",
        "",
        &device_b_key,
        &device_b_kid,
    );
    let response = app.oneshot(req).await.expect("response");
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

// =========================================================================
// 3. Trust Boundary: Unknown Device Key
// =========================================================================

/// Attack: Send a signed request using an Ed25519 key that has never been
/// registered as a device for any account.
///
/// Expected: 401 Unauthorized — the server must look up the KID in the database
/// and reject requests from unknown keys. If this test fails, the server is not
/// verifying device registration; any holder of an Ed25519 keypair could
/// generate a valid-looking KID and authenticate without ever registering.
#[shared_runtime_test]
async fn test_unknown_device_key_rejected() {
    // Sign up a real user so the DB is populated, then authenticate with a
    // completely unregistered key.
    let (_app, _keys, db) = signup_user("adv_unknown_key").await;
    let app = TestAppBuilder::new()
        .with_identity_pool(db.pool().clone())
        .build();

    let unknown_key = SigningKey::generate(&mut OsRng);
    let unknown_pubkey_bytes = unknown_key.verifying_key().to_bytes();
    let unknown_kid = Kid::derive(&unknown_pubkey_bytes);

    let req = build_authed_request(
        Method::GET,
        "/auth/devices",
        "",
        &unknown_key,
        &unknown_kid,
    );
    let response = app.oneshot(req).await.expect("response");
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

// =========================================================================
// 4. Trust Boundary: Signature Integrity (Body and Path)
// =========================================================================

/// Attack: Sign a request over one JSON body, then send the request with a
/// different body (same auth headers, different content).
///
/// Expected: 401 Unauthorized — the canonical message includes SHA-256 of the
/// request body. Modifying the body after signing produces a different hash,
/// invalidating the signature. If this test fails, an on-path attacker could
/// intercept and modify the request body without invalidating the authentication
/// headers, enabling payload injection (e.g., changing device names or pubkeys).
#[shared_runtime_test]
async fn test_tampered_body_rejected() {
    let (app, keys, _db) = signup_user("adv_tampered_body").await;

    // Build a POST /auth/devices body and sign it.
    let new_device_key = SigningKey::generate(&mut OsRng);
    let new_device_pubkey_bytes = new_device_key.verifying_key().to_bytes();
    let new_device_pubkey = encode_base64url(&new_device_pubkey_bytes);
    let cert = keys.root_signing_key.sign(&new_device_pubkey_bytes);

    let original_body = serde_json::json!({
        "pubkey": new_device_pubkey,
        "name": "Original Name",
        "certificate": encode_base64url(&cert.to_bytes()),
    })
    .to_string();

    // Produce auth headers signed for the original body.
    let timestamp = chrono::Utc::now().timestamp();
    let body_hash = Sha256::digest(original_body.as_bytes());
    let body_hash_hex = format!("{body_hash:x}");
    let canonical = format!("POST\n/auth/devices\n{timestamp}\n{body_hash_hex}");
    let signature = keys.device_signing_key.sign(canonical.as_bytes());

    // Build an attacker-modified body (device name changed after signing).
    let tampered_body = serde_json::json!({
        "pubkey": new_device_pubkey,
        "name": "Attacker-Controlled Name",
        "certificate": encode_base64url(&cert.to_bytes()),
    })
    .to_string();

    let req = Request::builder()
        .method(Method::POST)
        .uri("/auth/devices")
        .header("X-Device-Kid", keys.device_kid.to_string())
        .header("X-Signature", encode_base64url(&signature.to_bytes()))
        .header("X-Timestamp", timestamp.to_string())
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(tampered_body))
        .expect("request");

    let response = app.oneshot(req).await.expect("response");
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

/// Attack: Sign a request for path `/auth/devices` (no query string), then
/// send the request to `/auth/devices?admin=true` (injected query parameter).
///
/// Expected: 401 Unauthorized — the server includes the full path+query in the
/// canonical message (per auth.rs line 154: `path_and_query()`). If this test
/// fails, query parameter injection is possible: an attacker could append query
/// parameters that modify server-side behavior while re-using a legitimately
/// signed request (privilege escalation vector for any endpoint that dispatches
/// on query params).
#[shared_runtime_test]
async fn test_tampered_path_rejected() {
    let (app, keys, _db) = signup_user("adv_tampered_path").await;

    // Sign for the canonical path with no query string.
    let timestamp = chrono::Utc::now().timestamp();
    let body_hash = Sha256::digest(b"");
    let body_hash_hex = format!("{body_hash:x}");
    let canonical = format!("GET\n/auth/devices\n{timestamp}\n{body_hash_hex}");
    let signature = keys.device_signing_key.sign(canonical.as_bytes());

    // Send to a different URI with an injected query parameter.
    let req = Request::builder()
        .method(Method::GET)
        .uri("/auth/devices?admin=true")
        .header("X-Device-Kid", keys.device_kid.to_string())
        .header("X-Signature", encode_base64url(&signature.to_bytes()))
        .header("X-Timestamp", timestamp.to_string())
        .body(Body::empty())
        .expect("request");

    let response = app.oneshot(req).await.expect("response");
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

// =========================================================================
// 5. Trust Boundary: Timestamp Attacks
// =========================================================================

/// Attack: Submit a request with a timestamp more than 300 seconds in the future.
///
/// Expected: 401 Unauthorized — future timestamps outside the allowed skew window
/// must be rejected. If this test fails, an attacker could pre-sign requests for
/// future delivery, maintaining access after a device is revoked (the revocation
/// happens between signing and delivery) or creating long-lived signed tokens
/// that bypass the rolling replay-protection window.
#[shared_runtime_test]
async fn test_future_timestamp_rejected() {
    let (app, keys, _db) = signup_user("adv_future_ts").await;

    let future_timestamp = chrono::Utc::now().timestamp() + 600; // 10 min ahead
    let body_hash = Sha256::digest(b"");
    let body_hash_hex = format!("{body_hash:x}");
    let canonical = format!("GET\n/auth/devices\n{future_timestamp}\n{body_hash_hex}");
    let signature = keys.device_signing_key.sign(canonical.as_bytes());

    let req = Request::builder()
        .method(Method::GET)
        .uri("/auth/devices")
        .header("X-Device-Kid", keys.device_kid.to_string())
        .header("X-Signature", encode_base64url(&signature.to_bytes()))
        .header("X-Timestamp", future_timestamp.to_string())
        .body(Body::empty())
        .expect("request");

    let response = app.oneshot(req).await.expect("response");
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

/// Attack: Submit a request with a timestamp more than 300 seconds in the past.
///
/// Expected: 401 Unauthorized — stale timestamps outside the allowed skew window
/// must be rejected. If this test fails, an attacker who captured a valid signed
/// request could replay it minutes or hours later. Nonces expire alongside the
/// timestamp window, so a captured request outside the nonce retention window
/// could be replayed indefinitely.
#[shared_runtime_test]
async fn test_far_past_timestamp_rejected() {
    let (app, keys, _db) = signup_user("adv_past_ts").await;

    let past_timestamp = chrono::Utc::now().timestamp() - 600; // 10 min ago
    let body_hash = Sha256::digest(b"");
    let body_hash_hex = format!("{body_hash:x}");
    let canonical = format!("GET\n/auth/devices\n{past_timestamp}\n{body_hash_hex}");
    let signature = keys.device_signing_key.sign(canonical.as_bytes());

    let req = Request::builder()
        .method(Method::GET)
        .uri("/auth/devices")
        .header("X-Device-Kid", keys.device_kid.to_string())
        .header("X-Signature", encode_base64url(&signature.to_bytes()))
        .header("X-Timestamp", past_timestamp.to_string())
        .body(Body::empty())
        .expect("request");

    let response = app.oneshot(req).await.expect("response");
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

// =========================================================================
// 6. Trust Boundary: Missing / Malformed Auth Headers
// =========================================================================

/// Attack: Send a request to a device-authenticated endpoint with no auth headers.
///
/// Expected: 401 Unauthorized — all three headers (X-Device-Kid, X-Signature,
/// X-Timestamp) are required. If this test fails, the endpoint is not enforcing
/// authentication, allowing unauthenticated access to device management operations
/// and exposing account data to any caller.
#[shared_runtime_test]
async fn test_missing_auth_headers_rejected() {
    let (_app, _keys, db) = signup_user("adv_no_headers").await;
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

/// Attack: Include the X-Signature header but set it to an empty string.
///
/// Expected: 401 Unauthorized — an empty value decodes to zero bytes, which
/// cannot be a valid 64-byte Ed25519 signature. If this test fails, the server
/// is not validating minimum signature size before attempting verification,
/// potentially accepting trivially malformed auth headers.
#[shared_runtime_test]
async fn test_empty_signature_rejected() {
    let (app, keys, _db) = signup_user("adv_empty_sig").await;

    let timestamp = chrono::Utc::now().timestamp();

    let req = Request::builder()
        .method(Method::GET)
        .uri("/auth/devices")
        .header("X-Device-Kid", keys.device_kid.to_string())
        .header("X-Signature", "")
        .header("X-Timestamp", timestamp.to_string())
        .body(Body::empty())
        .expect("request");

    let response = app.oneshot(req).await.expect("response");
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

// =========================================================================
// 7. Trust Boundary: Cross-Account Certificate on Add-Device
// =========================================================================

/// Attack: Account A authenticates successfully, then submits a POST /auth/devices
/// request with a device certificate signed by Account B's root key.
///
/// Expected: 400 Bad Request — when adding a new device, the certificate must
/// verify against the authenticated account's root key stored in the database
/// (Account A's). If this test fails, an attacker with any legitimate account
/// could register device keys under other accounts by signing certificates with
/// their own root key, effectively taking over arbitrary accounts.
#[shared_runtime_test]
async fn test_cross_account_certificate_on_add_device_rejected() {
    let (app, keys_a, db) = signup_user("adv_cross_add_a").await;
    let (_app_b, keys_b) = signup_user_in_pool("adv_cross_add_b", db.pool()).await;

    // Generate a device key to register under Account A.
    let new_device_key = SigningKey::generate(&mut OsRng);
    let new_device_pubkey_bytes = new_device_key.verifying_key().to_bytes();
    let new_device_pubkey = encode_base64url(&new_device_pubkey_bytes);

    // Sign the device certificate with Account B's root key — cross-account forgery.
    let cross_cert = keys_b.root_signing_key.sign(&new_device_pubkey_bytes);
    let certificate = encode_base64url(&cross_cert.to_bytes());

    let body = serde_json::json!({
        "pubkey": new_device_pubkey,
        "name": "Cross-Account Device",
        "certificate": certificate,
    })
    .to_string();

    // Account A is authenticated, but the cert was signed by Account B's root key.
    let req = build_authed_request(
        Method::POST,
        "/auth/devices",
        &body,
        &keys_a.device_signing_key,
        &keys_a.device_kid,
    );

    let response = app.oneshot(req).await.expect("response");
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}
