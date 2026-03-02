//! Adversarial integration tests — trust-boundary focus.
//!
//! These tests probe the cryptographic trust boundary between client and server.
//! The server is a "dumb witness": it validates signatures and envelope structure,
//! but must never handle plaintext key material or allow unverified keys to act.
//!
//! Tests in this file are *additive* — they do not duplicate cases already covered
//! in `device_handler_tests.rs` or `identity_handler_tests.rs`.
//!
//! Focus area: trust-boundary
//! Run with: `cargo test --test adversarial_tests -- --test-threads=1`

mod common;

use axum::{
    body::Body,
    http::{header::CONTENT_TYPE, Method, Request, StatusCode},
};
use common::app_builder::TestAppBuilder;
use common::factories::{
    build_authed_request, sign_request, signup_user, signup_user_in_pool, valid_signup_with_keys,
};
use ed25519_dalek::{Signer, SigningKey};
use rand::rngs::OsRng;
use sha2::{Digest, Sha256};
use tc_crypto::{encode_base64url, BackupEnvelope, Kid};
use tc_test_macros::shared_runtime_test;
use tower::ServiceExt;

// =========================================================================
// Trust Boundary: Signup — certificate forgery (HTTP integration level)
// =========================================================================

/// Attack: POST /auth/signup with a device certificate signed by a random key,
/// not the account's root key.
///
/// Expected: 400 Bad Request — the service must verify that the certificate was
/// produced by the root key included in the same request. If this test fails,
/// an attacker could register an account with an uncertified device key, breaking
/// the chain of trust from root key → device key.
#[shared_runtime_test]
async fn test_tb_signup_forged_certificate_http() {
    // Use with_mocks() — cert validation fails before any DB write, so no real
    // DB is required for this test.
    let app = TestAppBuilder::with_mocks().build();

    let root_signing_key = SigningKey::generate(&mut OsRng);
    let root_pubkey_bytes = root_signing_key.verifying_key().to_bytes();
    let root_pubkey = encode_base64url(&root_pubkey_bytes);

    let device_signing_key = SigningKey::generate(&mut OsRng);
    let device_pubkey_bytes = device_signing_key.verifying_key().to_bytes();
    let device_pubkey = encode_base64url(&device_pubkey_bytes);

    // Forge the certificate: sign device pubkey with a random key, not the root key.
    let forger_key = SigningKey::generate(&mut OsRng);
    let forged_cert = forger_key.sign(&device_pubkey_bytes);
    let certificate = encode_base64url(&forged_cert.to_bytes());

    let envelope = BackupEnvelope::build([0xAA; 16], 65536, 3, 1, [0xBB; 12], &[0xCC; 48])
        .expect("test envelope");
    let backup_blob = encode_base64url(envelope.as_bytes());

    let json = serde_json::json!({
        "username": "adv_forged_cert_signup",
        "root_pubkey": root_pubkey,
        "backup": {"encrypted_blob": backup_blob},
        "device": {
            "pubkey": device_pubkey,
            "name": "Forged Cert Device",
            "certificate": certificate
        }
    })
    .to_string();

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
// Trust Boundary: Add-device — cross-account certificate
// =========================================================================

/// Attack: Account A is authenticated. A new device key is generated, but its
/// certificate is produced by Account B's root key (not Account A's). The request
/// is submitted to POST /auth/devices as Account A.
///
/// Expected: 400 Bad Request — the server must verify the certificate against the
/// *authenticated account's* root key, not just any root key. If this test fails,
/// an attacker who controls Account B could issue certificates that authorize
/// devices on Account A, enabling a cross-account device injection attack.
#[shared_runtime_test]
async fn test_tb_cross_account_certificate_on_add_device() {
    let (app, keys_a, db) = signup_user("adv_cross_cert_a").await;
    let (_app_b, keys_b) = signup_user_in_pool("adv_cross_cert_b", db.pool()).await;

    // New device key whose certificate is signed by Account B's root, not A's.
    let new_device_key = SigningKey::generate(&mut OsRng);
    let new_device_pubkey = new_device_key.verifying_key().to_bytes();
    let cross_cert = keys_b.root_signing_key.sign(&new_device_pubkey);

    let body = serde_json::json!({
        "pubkey": encode_base64url(&new_device_pubkey),
        "name": "Cross-Account Device",
        "certificate": encode_base64url(&cross_cert.to_bytes()),
    })
    .to_string();

    // Submitted as Account A — cert signed by B's root must not be accepted.
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

// =========================================================================
// Trust Boundary: Add-device — bit-flipped certificate
// =========================================================================

/// Attack: A valid certificate (root key signing device pubkey) has exactly one
/// bit flipped before being submitted to POST /auth/devices.
///
/// Expected: 400 Bad Request — the Ed25519 signature verification must reject
/// any certificate that does not exactly match the original signature. If this
/// test fails, the server is accepting corrupted or partially-modified certificates,
/// meaning the signature check provides no integrity guarantee.
#[shared_runtime_test]
async fn test_tb_bit_flipped_certificate_on_add_device() {
    let (app, keys, _db) = signup_user("adv_bit_flip_cert").await;

    let new_device_key = SigningKey::generate(&mut OsRng);
    let new_device_pubkey = new_device_key.verifying_key().to_bytes();

    // Generate a valid certificate, then flip the first bit.
    let valid_cert_sig = keys.root_signing_key.sign(&new_device_pubkey);
    let mut cert_bytes = valid_cert_sig.to_bytes().to_vec();
    cert_bytes[0] ^= 0x01; // single-bit corruption

    let body = serde_json::json!({
        "pubkey": encode_base64url(&new_device_pubkey),
        "name": "Bit Flip Device",
        "certificate": encode_base64url(&cert_bytes),
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

// =========================================================================
// Trust Boundary: Authenticated device endpoint — unregistered KID
// =========================================================================

/// Attack: A fresh Ed25519 key pair is generated client-side and used to sign
/// an authenticated request. The derived KID is not registered in the database
/// for any account.
///
/// Expected: 401 Unauthorized — the server must reject requests whose KID does
/// not correspond to any registered device key. If this test fails, an attacker
/// with an unregistered key could reach authenticated handlers, bypassing the
/// device registration requirement entirely.
#[shared_runtime_test]
async fn test_tb_request_signed_by_unknown_kid() {
    // Use a real DB (the auth path does a DB lookup on the KID at step 11 of auth).
    let (app, _keys, _db) = signup_user("adv_unknown_kid").await;

    // Generate a key pair that has never been registered.
    let unknown_key = SigningKey::generate(&mut OsRng);
    let unknown_kid = Kid::derive(&unknown_key.verifying_key().to_bytes());

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
// Trust Boundary: Request — body tampered after signing
// =========================================================================

/// Attack: The canonical message is computed and signed over the original request
/// body, but the body bytes sent in the HTTP request are different from those
/// that were signed (e.g., injecting a modified JSON payload after signing).
///
/// Expected: 401 Unauthorized — the server recomputes SHA-256 of the received body
/// and verifies it against the signature. If the body changed, the hashes differ,
/// the canonical messages differ, and verification must fail. If this test fails,
/// an attacker could intercept a signed request and swap its payload, executing
/// arbitrary device operations under a victim's signature.
#[shared_runtime_test]
async fn test_tb_tampered_body_after_signing() {
    let (app, keys, _db) = signup_user("adv_tampered_body").await;

    // Sign headers over the *original* body.
    let original_body = r#"{"pubkey":"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=","name":"Original","certificate":"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"}"#;
    let auth_headers = sign_request(
        "POST",
        "/auth/devices",
        original_body.as_bytes(),
        &keys.device_signing_key,
        &keys.device_kid,
    );

    // Build the request with a *different* body but the same auth headers.
    let tampered_body = r#"{"pubkey":"BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB","name":"Tampered","certificate":"BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB="}"#;
    let mut builder = Request::builder()
        .method(Method::POST)
        .uri("/auth/devices")
        .header(CONTENT_TYPE, "application/json");
    for (name, value) in &auth_headers {
        builder = builder.header(*name, value.as_str());
    }
    let req = builder
        .body(Body::from(tampered_body))
        .expect("request");

    let response = app.oneshot(req).await.expect("response");
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

// =========================================================================
// Trust Boundary: Request — path tampered after signing (query injection)
// =========================================================================

/// Attack: A request is signed for path `/auth/devices` (no query parameters).
/// The same auth headers are then sent with the request URI
/// `/auth/devices?admin=true`, injecting a query parameter that was not part
/// of the signed canonical message.
///
/// Expected: 401 Unauthorized — the server includes `path_and_query()` in the
/// canonical message, so the signed `/auth/devices` and the actual
/// `/auth/devices?admin=true` produce different canonical strings. If this test
/// fails, an attacker could inject arbitrary query parameters into a signed
/// request — critical for any future endpoints that gate behavior on query params.
#[shared_runtime_test]
async fn test_tb_tampered_path_after_signing() {
    // Build a fresh app sharing the same pool so the device key lookup works.
    let (_app, keys, db) = signup_user("adv_tampered_path").await;
    let app = TestAppBuilder::new()
        .with_identity_pool(db.pool().clone())
        .build();

    // Sign for /auth/devices (no query string).
    let auth_headers = sign_request(
        "GET",
        "/auth/devices",
        b"",
        &keys.device_signing_key,
        &keys.device_kid,
    );

    // Send to /auth/devices?admin=true — path+query in canonical won't match.
    let mut builder = Request::builder()
        .method(Method::GET)
        .uri("/auth/devices?admin=true");
    for (name, value) in &auth_headers {
        builder = builder.header(*name, value.as_str());
    }
    let req = builder.body(Body::empty()).expect("request");

    let response = app.oneshot(req).await.expect("response");
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

// =========================================================================
// Trust Boundary: Request — future timestamp outside the allowed skew window
// =========================================================================

/// Attack: A request is submitted with a timestamp 10 minutes in the future,
/// well outside the ±300-second allowed clock skew.
///
/// Expected: 401 Unauthorized — the server must enforce the timestamp skew limit
/// in both directions (past and future). If the future direction is not checked,
/// an attacker could pre-sign requests that remain valid for an arbitrarily long
/// window, effectively creating long-lived credentials from short-lived keys.
///
/// Uses `with_mocks()` because the timestamp check fires at step 6 of auth,
/// before the DB device lookup at step 12 — no real database is required.
#[shared_runtime_test]
async fn test_tb_future_timestamp_rejected() {
    // Timestamp rejection fires before the DB lookup, so no real DB is needed.
    let app = TestAppBuilder::with_mocks().build();

    let signing_key = SigningKey::generate(&mut OsRng);
    let kid = Kid::derive(&signing_key.verifying_key().to_bytes());

    let future_timestamp = chrono::Utc::now().timestamp() + 600; // 10 minutes in the future
    let nonce = uuid::Uuid::new_v4().to_string();
    let body_hash = Sha256::digest(b"");
    let body_hash_hex = format!("{body_hash:x}");
    let canonical =
        format!("GET\n/auth/devices\n{future_timestamp}\n{nonce}\n{body_hash_hex}");
    let signature = signing_key.sign(canonical.as_bytes());

    let req = Request::builder()
        .method(Method::GET)
        .uri("/auth/devices")
        .header("X-Device-Kid", kid.to_string())
        .header("X-Signature", encode_base64url(&signature.to_bytes()))
        .header("X-Timestamp", future_timestamp.to_string())
        .header("X-Nonce", &nonce)
        .body(Body::empty())
        .expect("request");

    let response = app.oneshot(req).await.expect("response");
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

// =========================================================================
// Trust Boundary: Request — empty X-Signature header
// =========================================================================

/// Attack: A request is sent to an authenticated device endpoint with the
/// X-Signature header present but containing an empty string.
///
/// Expected: 401 Unauthorized — decoding an empty string produces zero bytes,
/// which cannot be coerced to a 64-byte Ed25519 signature. The server must reject
/// the request before reaching signature verification. If this test fails, an
/// attacker might bypass the signature check with a crafted empty-value header,
/// a common HTTP header injection pattern.
///
/// Uses `with_mocks()` because the signature-size check fires at step 7 of auth,
/// before the DB device lookup at step 12 — no real database is required.
#[shared_runtime_test]
async fn test_tb_empty_signature_rejected() {
    // Signature-size rejection fires before the DB lookup, so no real DB is needed.
    let app = TestAppBuilder::with_mocks().build();

    let kid = Kid::derive(&[0u8; 32]); // arbitrary KID — never reaches DB lookup
    let timestamp = chrono::Utc::now().timestamp();
    let nonce = uuid::Uuid::new_v4().to_string();

    let req = Request::builder()
        .method(Method::GET)
        .uri("/auth/devices")
        .header("X-Device-Kid", kid.to_string())
        .header("X-Signature", "") // empty — decodes to 0 bytes, not 64
        .header("X-Timestamp", timestamp.to_string())
        .header("X-Nonce", &nonce)
        .body(Body::empty())
        .expect("request");

    let response = app.oneshot(req).await.expect("response");
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

// =========================================================================
// Trust Boundary: Request — HTTP method included in canonical message
// =========================================================================

/// Attack: A request is signed for HTTP method GET on `/auth/devices`, but the
/// same auth headers are reused on a POST request to the same path.
///
/// Expected: 401 Unauthorized — the canonical message includes the HTTP method,
/// so a signature over "GET\n..." cannot verify against "POST\n...". If this test
/// fails, a single signed GET request could be replayed as a state-mutating POST
/// (or DELETE), allowing an attacker to trigger device creation or revocation using
/// a signature that was only authorized for a read operation.
#[shared_runtime_test]
async fn test_tb_method_mismatch_rejected() {
    let (_app, keys, db) = signup_user("adv_method_mismatch").await;
    let app = TestAppBuilder::new()
        .with_identity_pool(db.pool().clone())
        .build();

    // Sign for GET /auth/devices.
    let auth_headers = sign_request(
        "GET",
        "/auth/devices",
        b"",
        &keys.device_signing_key,
        &keys.device_kid,
    );

    // Send as POST /auth/devices with the GET signature — canonical mismatch.
    let mut builder = Request::builder()
        .method(Method::POST)
        .uri("/auth/devices")
        .header(CONTENT_TYPE, "application/json");
    for (name, value) in &auth_headers {
        builder = builder.header(*name, value.as_str());
    }
    // Minimal body so the POST route is reached (POST /auth/devices requires JSON).
    let req = builder
        .body(Body::from(r#"{"pubkey":"","name":"","certificate":""}"#))
        .expect("request");

    let response = app.oneshot(req).await.expect("response");
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
