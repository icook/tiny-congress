//! Adversarial integration tests for TinyCongress identity system.
//!
//! Three focus areas:
//!
//! 1. **Trust Boundary** — certificate forgery, auth bypass, replay/tamper attacks
//! 2. **API Robustness** — malformed/boundary inputs, backup envelope validation
//! 3. **Domain Logic** — max-device limit, revocation lifecycle, cross-account isolation
//!
//! Run with:
//! ```bash
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
use tc_crypto::{encode_base64url, Kid};
use tc_test_macros::shared_runtime_test;
use tower::ServiceExt;

// =========================================================================
// Shared helpers — mirroring device_handler_tests.rs patterns
// =========================================================================

/// Sign a request at a specific timestamp (used to forge future/past timestamps).
fn sign_request_at_timestamp(
    method: &str,
    path: &str,
    body: &[u8],
    signing_key: &SigningKey,
    kid: &Kid,
    timestamp: i64,
) -> Vec<(&'static str, String)> {
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

/// Sign a request at the current wall-clock time.
fn sign_request(
    method: &str,
    path: &str,
    body: &[u8],
    signing_key: &SigningKey,
    kid: &Kid,
) -> Vec<(&'static str, String)> {
    sign_request_at_timestamp(
        method,
        path,
        body,
        signing_key,
        kid,
        chrono::Utc::now().timestamp(),
    )
}

/// Build a fully signed `Request<Body>` for an authenticated device endpoint.
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

/// Sign up a new user in a fresh isolated database; returns the router, keys, and DB handle.
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

    assert_eq!(
        response.status(),
        StatusCode::CREATED,
        "signup for {username} failed"
    );

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

    assert_eq!(
        response.status(),
        StatusCode::CREATED,
        "signup for {username} failed"
    );

    (app, keys)
}

/// Build a raw backup envelope byte slice with arbitrary header fields.
///
/// Layout: [version(1), kdf(1), m_cost(4 LE), t_cost(4 LE), p_cost(4 LE),
///          salt(16), nonce(12), ciphertext(48)]  = 90 bytes total
///
/// Used to construct envelopes with unsupported versions, KDFs, or weak params
/// without going through `BackupEnvelope::build`, which enforces the minimums.
fn raw_envelope(version: u8, kdf: u8, m_cost: u32, t_cost: u32, p_cost: u32) -> Vec<u8> {
    let mut raw = vec![0x00u8; 90];
    raw[0] = version;
    raw[1] = kdf;
    raw[2..6].copy_from_slice(&m_cost.to_le_bytes());
    raw[6..10].copy_from_slice(&t_cost.to_le_bytes());
    raw[10..14].copy_from_slice(&p_cost.to_le_bytes());
    // salt (bytes 14..30), nonce (30..42), ciphertext (42..90) remain zero
    raw
}

// =========================================================================
// Section 1: Trust Boundary Probing
// =========================================================================

/// Attack: Submit a signup with a device certificate signed by a random key, not the
/// account's root key.
///
/// Expected: 400 Bad Request — the server must verify the certificate was signed by the
/// root key submitted in the same request. If this test fails, an attacker could register
/// arbitrary device keys for any account without possessing the root private key.
#[shared_runtime_test]
async fn test_signup_forged_device_certificate() {
    let app = TestAppBuilder::with_mocks().build();

    let root_signing_key = SigningKey::generate(&mut OsRng);
    let root_pubkey_bytes = root_signing_key.verifying_key().to_bytes();

    let device_key = SigningKey::generate(&mut OsRng);
    let device_pubkey_bytes = device_key.verifying_key().to_bytes();

    // Forger signs the device pubkey with a completely unrelated key
    let forger_key = SigningKey::generate(&mut OsRng);
    let bad_cert = forger_key.sign(&device_pubkey_bytes);

    let envelope = tc_crypto::BackupEnvelope::build(
        [0xAA; 16],
        65536,
        3,
        1,
        [0xBB; 12],
        &[0xCC; 48],
    )
    .expect("test envelope");

    let body = serde_json::json!({
        "username": "adv_forged_cert",
        "root_pubkey": encode_base64url(&root_pubkey_bytes),
        "backup": { "encrypted_blob": encode_base64url(envelope.as_bytes()) },
        "device": {
            "pubkey": encode_base64url(&device_pubkey_bytes),
            "name": "Forged",
            "certificate": encode_base64url(&bad_cert.to_bytes()),
        }
    })
    .to_string();

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

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

/// Attack: Submit a signup where the device certificate was signed by Account B's root key,
/// but Account A's root key is in the `root_pubkey` field.
///
/// Expected: 400 Bad Request — the certificate must be signed by the exact root key
/// submitted in the request. Cross-account certificate re-use must be detected and
/// rejected. If this test fails, any account holder could issue device certificates for
/// other accounts' device keys.
#[shared_runtime_test]
async fn test_signup_cross_account_certificate() {
    let app = TestAppBuilder::with_mocks().build();

    // Account A's root key — submitted as root_pubkey in this request
    let root_key_a = SigningKey::generate(&mut OsRng);
    let root_pubkey_a_bytes = root_key_a.verifying_key().to_bytes();

    // Account B's root key — used to sign the certificate (the attacker's key)
    let root_key_b = SigningKey::generate(&mut OsRng);

    let device_key = SigningKey::generate(&mut OsRng);
    let device_pubkey_bytes = device_key.verifying_key().to_bytes();

    // Certificate signed by B's root, not A's
    let cert_by_b = root_key_b.sign(&device_pubkey_bytes);

    let envelope = tc_crypto::BackupEnvelope::build(
        [0xAA; 16],
        65536,
        3,
        1,
        [0xBB; 12],
        &[0xCC; 48],
    )
    .expect("test envelope");

    let body = serde_json::json!({
        "username": "adv_cross_acct_cert",
        "root_pubkey": encode_base64url(&root_pubkey_a_bytes),
        "backup": { "encrypted_blob": encode_base64url(envelope.as_bytes()) },
        "device": {
            "pubkey": encode_base64url(&device_pubkey_bytes),
            "name": "Wrong Account Device",
            "certificate": encode_base64url(&cert_by_b.to_bytes()),
        }
    })
    .to_string();

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

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

/// Attack: Take a valid device certificate and flip one bit before submission.
///
/// Expected: 400 Bad Request — Ed25519 signature verification is all-or-nothing; a
/// single-bit change in the certificate bytes must invalidate it. If this test fails,
/// the server either does not verify signatures or uses a tolerant (non-standard)
/// verification algorithm.
#[shared_runtime_test]
async fn test_signup_bit_flipped_certificate() {
    let app = TestAppBuilder::with_mocks().build();

    let root_signing_key = SigningKey::generate(&mut OsRng);
    let root_pubkey_bytes = root_signing_key.verifying_key().to_bytes();

    let device_key = SigningKey::generate(&mut OsRng);
    let device_pubkey_bytes = device_key.verifying_key().to_bytes();

    let valid_cert = root_signing_key.sign(&device_pubkey_bytes);
    let mut cert_bytes = valid_cert.to_bytes();

    // Flip the first bit
    cert_bytes[0] ^= 0x01;

    let envelope = tc_crypto::BackupEnvelope::build(
        [0xAA; 16],
        65536,
        3,
        1,
        [0xBB; 12],
        &[0xCC; 48],
    )
    .expect("test envelope");

    let body = serde_json::json!({
        "username": "adv_bit_flip_cert",
        "root_pubkey": encode_base64url(&root_pubkey_bytes),
        "backup": { "encrypted_blob": encode_base64url(envelope.as_bytes()) },
        "device": {
            "pubkey": encode_base64url(&device_pubkey_bytes),
            "name": "Bit Flip Device",
            "certificate": encode_base64url(&cert_bytes),
        }
    })
    .to_string();

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

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

/// Attack: After revoking a device, use that device's private key to send an authenticated
/// request (valid signature, but revoked device).
///
/// Expected: 403 Forbidden — the server must check revocation status and reject requests
/// from revoked devices even when the signature is cryptographically valid. If this test
/// fails, an attacker who obtains stolen key material can continue to authenticate after
/// the legitimate owner has revoked the device.
#[shared_runtime_test]
async fn test_revoked_device_auth_forbidden() {
    let (app, keys, _db) = signup_user("adv_revoked_auth").await;

    // Add a second device that will be revoked
    let second_key = SigningKey::generate(&mut OsRng);
    let second_pubkey = second_key.verifying_key().to_bytes();
    let cert = keys.root_signing_key.sign(&second_pubkey);
    let second_kid = Kid::derive(&second_pubkey);

    let add_body = serde_json::json!({
        "pubkey": encode_base64url(&second_pubkey),
        "name": "Soon Revoked",
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
    let add_resp = app.clone().oneshot(add_req).await.expect("response");
    assert_eq!(add_resp.status(), StatusCode::CREATED);

    // Revoke the second device using the first
    let revoke_path = format!("/auth/devices/{second_kid}");
    let revoke_req = build_authed_request(
        Method::DELETE,
        &revoke_path,
        "",
        &keys.device_signing_key,
        &keys.device_kid,
    );
    let revoke_resp = app.clone().oneshot(revoke_req).await.expect("response");
    assert_eq!(revoke_resp.status(), StatusCode::NO_CONTENT);

    // Attempt to authenticate with the now-revoked device key
    let auth_req = build_authed_request(
        Method::GET,
        "/auth/devices",
        "",
        &second_key,
        &second_kid,
    );
    let auth_resp = app.oneshot(auth_req).await.expect("response");
    assert_eq!(auth_resp.status(), StatusCode::FORBIDDEN);
}

/// Attack: Sign a request using a key pair that was never registered as a device.
///
/// Expected: 401 Unauthorized — the server must reject requests from keys it has no
/// record of. If this test fails, any Ed25519 key pair could authenticate without
/// registration, completely bypassing the account model.
#[shared_runtime_test]
async fn test_request_signed_by_unknown_key() {
    // Mock repo returns NotFound for all get_device_key_by_kid calls
    let app = TestAppBuilder::with_mocks().build();

    let unknown_key = SigningKey::generate(&mut OsRng);
    let unknown_pubkey = unknown_key.verifying_key().to_bytes();
    let unknown_kid = Kid::derive(&unknown_pubkey);

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

/// Attack: Sign a POST /auth/devices request with body "A", then replace the body
/// with body "B" before sending.
///
/// Expected: 401 Unauthorized — the canonical message includes a SHA-256 hash of the
/// body. If the body is tampered after signing, the computed hash won't match the
/// signed hash and signature verification fails. If this test fails, an attacker could
/// reuse a captured signed request with a different payload (e.g. a different device
/// key than the one the legitimate user authorized).
#[shared_runtime_test]
async fn test_tampered_body_rejected() {
    let (app, keys, _db) = signup_user("adv_tamper_body").await;

    let original_body = r#"{"pubkey": "AAAA", "name": "original", "certificate": "CCCC"}"#;

    let headers = sign_request(
        "POST",
        "/auth/devices",
        original_body.as_bytes(),
        &keys.device_signing_key,
        &keys.device_kid,
    );

    // Send with a different body — the signature no longer covers this content
    let tampered_body = r#"{"pubkey": "ZZZZ", "name": "injected", "certificate": "YYYY"}"#;
    let mut builder = Request::builder()
        .method(Method::POST)
        .uri("/auth/devices")
        .header(CONTENT_TYPE, "application/json");

    for (name, value) in &headers {
        builder = builder.header(*name, value);
    }

    let req = builder.body(Body::from(tampered_body)).expect("request");
    let response = app.oneshot(req).await.expect("response");
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

/// Attack: Sign a request for path "/auth/devices" (no query string), then send the
/// request to "/auth/devices?injected=true" (with a query parameter appended).
///
/// Expected: 401 Unauthorized — the canonical message includes the full path+query
/// string. A signed path and the actual path must match exactly. If this test fails,
/// an attacker could inject query parameters (e.g. for cache poisoning, parameter
/// pollution, or admin-mode activation) without invalidating the signature.
#[shared_runtime_test]
async fn test_tampered_path_rejected() {
    let (app, keys, _db) = signup_user("adv_tamper_path").await;

    // Sign for the clean path (no query string)
    let headers = sign_request(
        "GET",
        "/auth/devices",
        b"",
        &keys.device_signing_key,
        &keys.device_kid,
    );

    // Send to a path with injected query parameters
    let mut builder = Request::builder()
        .method(Method::GET)
        .uri("/auth/devices?admin=true");

    for (name, value) in &headers {
        builder = builder.header(*name, value);
    }

    let req = builder.body(Body::empty()).expect("request");
    let response = app.oneshot(req).await.expect("response");
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

/// Attack: Send a request with X-Timestamp set to 301 seconds in the future (one second
/// beyond the +300s tolerance window).
///
/// Expected: 401 Unauthorized — the server enforces a ±300 second timestamp window.
/// Accepting timestamps far in the future would allow pre-computed signed requests to
/// be used after a key compromise is detected, undermining the time-bound freshness
/// guarantee.
#[shared_runtime_test]
async fn test_future_timestamp_rejected() {
    // Timestamp check happens before DB lookup — mock is sufficient
    let app = TestAppBuilder::with_mocks().build();

    let future_timestamp = chrono::Utc::now().timestamp() + 301;
    let key = SigningKey::generate(&mut OsRng);
    let kid = Kid::derive(&key.verifying_key().to_bytes());

    let headers = sign_request_at_timestamp("GET", "/auth/devices", b"", &key, &kid, future_timestamp);

    let mut builder = Request::builder().method(Method::GET).uri("/auth/devices");
    for (name, value) in &headers {
        builder = builder.header(*name, value);
    }

    let response = app
        .oneshot(builder.body(Body::empty()).expect("request"))
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

/// Attack: Send a device endpoint request with no X-Device-Kid header (but with the
/// other required headers).
///
/// Expected: 401 Unauthorized — all three auth headers are mandatory. Accepting
/// partial headers could allow probing authentication state without full credentials.
#[shared_runtime_test]
async fn test_missing_device_kid_header() {
    let app = TestAppBuilder::with_mocks().build();

    let req = Request::builder()
        .method(Method::GET)
        .uri("/auth/devices")
        .header("X-Signature", encode_base64url(&[0u8; 64]))
        .header("X-Timestamp", chrono::Utc::now().timestamp().to_string())
        .body(Body::empty())
        .expect("request");

    let response = app.oneshot(req).await.expect("response");
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

/// Attack: Send a device endpoint request with no X-Signature header (but with the
/// other required headers).
///
/// Expected: 401 Unauthorized — without the signature there is no cryptographic proof
/// that the requester holds the device's private key.
#[shared_runtime_test]
async fn test_missing_signature_header() {
    let app = TestAppBuilder::with_mocks().build();

    // Use a syntactically valid 22-char KID
    let req = Request::builder()
        .method(Method::GET)
        .uri("/auth/devices")
        .header("X-Device-Kid", "cs1uhCLEB_ttCYaQ8RMLfQ")
        .header("X-Timestamp", chrono::Utc::now().timestamp().to_string())
        .body(Body::empty())
        .expect("request");

    let response = app.oneshot(req).await.expect("response");
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

/// Attack: Send a device endpoint request with no X-Timestamp header (but with the
/// other required headers).
///
/// Expected: 401 Unauthorized — without a timestamp the replay-protection nonce window
/// cannot function; the server must reject requests missing this time-binding component.
#[shared_runtime_test]
async fn test_missing_timestamp_header() {
    let app = TestAppBuilder::with_mocks().build();

    let req = Request::builder()
        .method(Method::GET)
        .uri("/auth/devices")
        .header("X-Device-Kid", "cs1uhCLEB_ttCYaQ8RMLfQ")
        .header("X-Signature", encode_base64url(&[0u8; 64]))
        .body(Body::empty())
        .expect("request");

    let response = app.oneshot(req).await.expect("response");
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

/// Attack: Send a device endpoint request with X-Signature set to an empty string.
///
/// Expected: 401 Unauthorized — an empty signature is not a valid 64-byte Ed25519
/// signature. The server must reject empty or truncated signatures rather than treating
/// "no signature" as equivalent to no authentication requirement.
#[shared_runtime_test]
async fn test_empty_signature_header() {
    let app = TestAppBuilder::with_mocks().build();

    let req = Request::builder()
        .method(Method::GET)
        .uri("/auth/devices")
        .header("X-Device-Kid", "cs1uhCLEB_ttCYaQ8RMLfQ")
        .header("X-Signature", "") // empty
        .header("X-Timestamp", chrono::Utc::now().timestamp().to_string())
        .body(Body::empty())
        .expect("request");

    let response = app.oneshot(req).await.expect("response");
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

// =========================================================================
// Section 2: API Robustness
// =========================================================================

/// Attack: Send a completely malformed JSON body to the signup endpoint.
///
/// Expected: 400 or 422 — the server must not panic or leak internal state when
/// receiving unparseable input. Axum returns 422 for JSON parse failures by default.
#[shared_runtime_test]
async fn test_signup_malformed_json() {
    let app = TestAppBuilder::with_mocks().build();

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/auth/signup")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from("{invalid json"))
                .expect("request"),
        )
        .await
        .expect("response");

    assert!(
        matches!(
            response.status(),
            StatusCode::BAD_REQUEST | StatusCode::UNPROCESSABLE_ENTITY
        ),
        "expected 400 or 422, got {}",
        response.status()
    );
}

/// Attack: POST to the signup endpoint with a completely empty request body.
///
/// Expected: 400 or 422 — an empty body is not valid JSON and must be rejected.
/// The server must not interpret absence of content as default/zero values.
#[shared_runtime_test]
async fn test_signup_empty_body() {
    let app = TestAppBuilder::with_mocks().build();

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/auth/signup")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert!(
        matches!(
            response.status(),
            StatusCode::BAD_REQUEST | StatusCode::UNPROCESSABLE_ENTITY
        ),
        "expected 400 or 422, got {}",
        response.status()
    );
}

/// Attack: Submit a signup with a 65-character username (one over the 64-character limit).
///
/// Expected: 400 Bad Request — the server must enforce the maximum username length
/// strictly. Accepting over-long usernames could cause storage anomalies or bypass
/// downstream length constraints.
#[shared_runtime_test]
async fn test_signup_oversized_username() {
    let app = TestAppBuilder::with_mocks().build();

    let (json_base, _) = valid_signup_with_keys("placeholder");
    let mut body: serde_json::Value = serde_json::from_str(&json_base).expect("json");
    body["username"] = serde_json::Value::String("a".repeat(65));

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/auth/signup")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string()))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

/// Attack: Submit a signup with an empty-string username.
///
/// Expected: 400 Bad Request — empty usernames must be rejected. Every account must
/// have an identifiable, non-empty username.
#[shared_runtime_test]
async fn test_signup_empty_username() {
    let app = TestAppBuilder::with_mocks().build();

    let (json_base, _) = valid_signup_with_keys("placeholder");
    let mut body: serde_json::Value = serde_json::from_str(&json_base).expect("json");
    body["username"] = serde_json::Value::String(String::new());

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/auth/signup")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string()))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

/// Attack: Submit a signup with a Unicode (non-ASCII) character in the username ("álice").
///
/// Expected: 400 Bad Request — usernames must be ASCII alphanumeric with hyphens and
/// underscores only. Unicode characters introduce homograph attack vectors (look-alike
/// characters) and encoding ambiguities that could enable impersonation.
#[shared_runtime_test]
async fn test_signup_unicode_username() {
    let app = TestAppBuilder::with_mocks().build();

    let (json_base, _) = valid_signup_with_keys("placeholder");
    let mut body: serde_json::Value = serde_json::from_str(&json_base).expect("json");
    // "álice" — contains U+00E1 (LATIN SMALL LETTER A WITH ACUTE)
    body["username"] = serde_json::Value::String("\u{00e1}lice".to_string());

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/auth/signup")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string()))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

/// Attack: Submit a signup with the reserved username "admin" (lowercase).
///
/// Expected: 400 Bad Request — reserved usernames must be blocked to prevent
/// impersonation of administrative or system accounts.
#[shared_runtime_test]
async fn test_signup_reserved_username_lowercase() {
    let app = TestAppBuilder::with_mocks().build();

    let (json_base, _) = valid_signup_with_keys("placeholder");
    let mut body: serde_json::Value = serde_json::from_str(&json_base).expect("json");
    body["username"] = serde_json::Value::String("admin".to_string());

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/auth/signup")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string()))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

/// Attack: Submit a signup with the reserved username "Admin" (mixed case).
///
/// Expected: 400 Bad Request — reserved username checks must be case-insensitive.
/// Accepting "Admin" while blocking "admin" would allow impersonation via
/// capitalization variation.
#[shared_runtime_test]
async fn test_signup_reserved_username_mixed_case() {
    let app = TestAppBuilder::with_mocks().build();

    let (json_base, _) = valid_signup_with_keys("placeholder");
    let mut body: serde_json::Value = serde_json::from_str(&json_base).expect("json");
    body["username"] = serde_json::Value::String("Admin".to_string());

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/auth/signup")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string()))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

/// Attack: Send an authenticated request with a 21-character KID in X-Device-Kid
/// (one character short of the required 22).
///
/// Expected: 401 Unauthorized — KIDs must be exactly 22 characters. A short KID is
/// malformed and must not match any legitimate device record.
#[shared_runtime_test]
async fn test_auth_kid_header_too_short() {
    let app = TestAppBuilder::with_mocks().build();

    let req = Request::builder()
        .method(Method::GET)
        .uri("/auth/devices")
        .header("X-Device-Kid", "a".repeat(21)) // 21 chars
        .header("X-Signature", encode_base64url(&[0u8; 64]))
        .header("X-Timestamp", chrono::Utc::now().timestamp().to_string())
        .body(Body::empty())
        .expect("request");

    let response = app.oneshot(req).await.expect("response");
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

/// Attack: Send an authenticated request with a 23-character KID in X-Device-Kid
/// (one character over the required 22).
///
/// Expected: 401 Unauthorized — KIDs must be exactly 22 characters. An over-long KID
/// is malformed and must be rejected before any lookup attempt.
#[shared_runtime_test]
async fn test_auth_kid_header_too_long() {
    let app = TestAppBuilder::with_mocks().build();

    let req = Request::builder()
        .method(Method::GET)
        .uri("/auth/devices")
        .header("X-Device-Kid", "a".repeat(23)) // 23 chars
        .header("X-Signature", encode_base64url(&[0u8; 64]))
        .header("X-Timestamp", chrono::Utc::now().timestamp().to_string())
        .body(Body::empty())
        .expect("request");

    let response = app.oneshot(req).await.expect("response");
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

/// Attack: Send an authenticated request with invalid characters in the X-Device-Kid
/// header (22 chars but contains '!', which is not in the base64url alphabet).
///
/// Expected: 401 Unauthorized — KIDs must contain only [A-Za-z0-9_-]. An invalid
/// character indicates a malformed or injected KID that should never match a legitimate
/// device record.
#[shared_runtime_test]
async fn test_auth_kid_header_invalid_chars() {
    let app = TestAppBuilder::with_mocks().build();

    // 22 chars but the last char is '!' (not base64url)
    let req = Request::builder()
        .method(Method::GET)
        .uri("/auth/devices")
        .header("X-Device-Kid", "abcdefghijklmnopqrstu!")
        .header("X-Signature", encode_base64url(&[0u8; 64]))
        .header("X-Timestamp", chrono::Utc::now().timestamp().to_string())
        .body(Body::empty())
        .expect("request");

    let response = app.oneshot(req).await.expect("response");
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

/// Attack: Submit a signup with a root public key that is 31 bytes (one byte short of
/// Ed25519's required 32 bytes).
///
/// Expected: 400 Bad Request — Ed25519 public keys are always exactly 32 bytes.
/// Accepting a 31-byte key would cause undefined behavior when the key material is
/// passed to the crypto primitives.
#[shared_runtime_test]
async fn test_signup_root_pubkey_wrong_size_short() {
    let app = TestAppBuilder::with_mocks().build();

    let (json_base, _) = valid_signup_with_keys("placeholder");
    let mut body: serde_json::Value = serde_json::from_str(&json_base).expect("json");
    body["root_pubkey"] = serde_json::Value::String(encode_base64url(&[0xABu8; 31]));

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/auth/signup")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string()))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

/// Attack: Submit a signup with a root public key that is 33 bytes (one byte over
/// Ed25519's required 32 bytes).
///
/// Expected: 400 Bad Request — no pubkey size other than exactly 32 bytes should be
/// accepted for Ed25519. An over-long key could be an attempt to append extra data
/// that influences downstream processing.
#[shared_runtime_test]
async fn test_signup_root_pubkey_wrong_size_long() {
    let app = TestAppBuilder::with_mocks().build();

    let (json_base, _) = valid_signup_with_keys("placeholder");
    let mut body: serde_json::Value = serde_json::from_str(&json_base).expect("json");
    body["root_pubkey"] = serde_json::Value::String(encode_base64url(&[0xABu8; 33]));

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/auth/signup")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string()))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

/// Attack: Submit a signup with a root public key field that is not valid base64url
/// (contains special characters that are not in the base64url alphabet).
///
/// Expected: 400 Bad Request — invalid encoding must be rejected before any decoding
/// is attempted. The server must never pass undecodable input to the crypto layer.
#[shared_runtime_test]
async fn test_signup_root_pubkey_invalid_encoding() {
    let app = TestAppBuilder::with_mocks().build();

    let (json_base, _) = valid_signup_with_keys("placeholder");
    let mut body: serde_json::Value = serde_json::from_str(&json_base).expect("json");
    body["root_pubkey"] = serde_json::Value::String("!!!not-base64url!!!".to_string());

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/auth/signup")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string()))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

/// Attack: Submit a signup with a device public key that is 16 bytes (half of Ed25519's 32).
///
/// Expected: 400 Bad Request — device pubkeys must be exactly 32 bytes. Shorter keys
/// could represent partial key material and must be rejected before KID derivation or
/// certificate verification.
#[shared_runtime_test]
async fn test_signup_device_pubkey_wrong_size() {
    let app = TestAppBuilder::with_mocks().build();

    let (json_base, _) = valid_signup_with_keys("placeholder");
    let mut body: serde_json::Value = serde_json::from_str(&json_base).expect("json");
    body["device"]["pubkey"] = serde_json::Value::String(encode_base64url(&[0xABu8; 16]));

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/auth/signup")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string()))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

/// Attack: Submit a signup with a device certificate that is 32 bytes (half of Ed25519's
/// required 64 bytes).
///
/// Expected: 400 Bad Request — Ed25519 signatures are always exactly 64 bytes. A
/// 32-byte value cannot be a valid signature and must be rejected before any
/// verification attempt.
#[shared_runtime_test]
async fn test_signup_certificate_wrong_size() {
    let app = TestAppBuilder::with_mocks().build();

    let (json_base, _) = valid_signup_with_keys("placeholder");
    let mut body: serde_json::Value = serde_json::from_str(&json_base).expect("json");
    body["device"]["certificate"] =
        serde_json::Value::String(encode_base64url(&[0xABu8; 32]));

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/auth/signup")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string()))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

/// Attack: Submit a signup with a backup envelope that is 89 bytes (one byte below the
/// 90-byte minimum: 42-byte header + 48-byte minimum ciphertext).
///
/// Expected: 400 Bad Request — an undersized envelope cannot contain valid header fields
/// and ciphertext. Accepting truncated envelopes could cause out-of-bounds reads when
/// parsing header fields.
#[shared_runtime_test]
async fn test_signup_backup_too_small() {
    let app = TestAppBuilder::with_mocks().build();

    let (json_base, _) = valid_signup_with_keys("placeholder");
    let mut body: serde_json::Value = serde_json::from_str(&json_base).expect("json");
    // 89 bytes = 1 under the 90-byte minimum
    body["backup"]["encrypted_blob"] =
        serde_json::Value::String(encode_base64url(&[0x01u8; 89]));

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/auth/signup")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string()))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

/// Attack: Submit a signup with a backup envelope that is 4097 bytes (one byte over the
/// 4096-byte maximum).
///
/// Expected: 400 Bad Request — oversized envelopes must be rejected. Very large envelopes
/// could be used to exhaust server memory or storage, or to bypass size-based validation
/// in storage backends.
#[shared_runtime_test]
async fn test_signup_backup_too_large() {
    let app = TestAppBuilder::with_mocks().build();

    let (json_base, _) = valid_signup_with_keys("placeholder");
    let mut body: serde_json::Value = serde_json::from_str(&json_base).expect("json");
    // 4097 bytes = 1 over the 4096-byte maximum
    body["backup"]["encrypted_blob"] =
        serde_json::Value::String(encode_base64url(&[0x01u8; 4097]));

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/auth/signup")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string()))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

/// Attack: Submit a signup with a backup envelope whose version byte is 0x02 (unsupported).
///
/// Expected: 400 Bad Request — only version 0x01 is accepted. Unrecognized envelope
/// versions must be rejected; the server cannot validate or store envelopes using
/// unknown format specifications.
#[shared_runtime_test]
async fn test_signup_backup_wrong_version() {
    let app = TestAppBuilder::with_mocks().build();

    let (json_base, _) = valid_signup_with_keys("placeholder");
    let mut body: serde_json::Value = serde_json::from_str(&json_base).expect("json");
    // version=0x02, kdf=0x01 (Argon2id), valid KDF params
    let raw = raw_envelope(0x02, 0x01, 65536, 3, 1);
    body["backup"]["encrypted_blob"] = serde_json::Value::String(encode_base64url(&raw));

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/auth/signup")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string()))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

/// Attack: Submit a signup with a backup envelope whose KDF byte is 0x02 (e.g., PBKDF2
/// or an unknown algorithm) instead of the required 0x01 (Argon2id).
///
/// Expected: 400 Bad Request — only Argon2id (KDF byte 0x01) is accepted. Unrecognized
/// KDF identifiers must be rejected to prevent weak or unknown KDFs from protecting
/// encrypted key material.
#[shared_runtime_test]
async fn test_signup_backup_wrong_kdf() {
    let app = TestAppBuilder::with_mocks().build();

    let (json_base, _) = valid_signup_with_keys("placeholder");
    let mut body: serde_json::Value = serde_json::from_str(&json_base).expect("json");
    // version=0x01, kdf=0x02 (unsupported), valid KDF params
    let raw = raw_envelope(0x01, 0x02, 65536, 3, 1);
    body["backup"]["encrypted_blob"] = serde_json::Value::String(encode_base64url(&raw));

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/auth/signup")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string()))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

/// Attack: Submit a signup with Argon2id KDF parameters well below OWASP 2024 minimums
/// (m_cost=1024, t_cost=1, p_cost=1).
///
/// Expected: 400 Bad Request — the server must enforce minimum Argon2id parameters
/// regardless of what the client submits. Accepting weak parameters means stored backups
/// could be brute-forced cheaply if the ciphertext is ever disclosed.
///
/// Severity: HIGH — if this test fails, backup envelopes may be stored with inadequate
/// KDF hardness, undermining the entire backup security model.
#[shared_runtime_test]
async fn test_signup_kdf_params_too_weak() {
    let app = TestAppBuilder::with_mocks().build();

    let (json_base, _) = valid_signup_with_keys("placeholder");
    let mut body: serde_json::Value = serde_json::from_str(&json_base).expect("json");
    // m_cost=1024 (far below 65536), t_cost=1 (below 3), p_cost=1 (at min)
    let raw = raw_envelope(0x01, 0x01, 1024, 1, 1);
    body["backup"]["encrypted_blob"] = serde_json::Value::String(encode_base64url(&raw));

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/auth/signup")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string()))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

/// Attack: Submit a signup with m_cost=65535 — exactly one below the minimum of 65536.
///
/// Expected: 400 Bad Request — the minimum is a strict lower bound (m_cost >= 65536).
/// An off-by-one acceptance would undermine the OWASP hardness guarantee. This boundary
/// test verifies that the check uses >= not >.
///
/// Severity: HIGH — demonstrates whether the server enforces the exact OWASP 2024
/// minimum or is susceptible to off-by-one bypass.
#[shared_runtime_test]
async fn test_signup_kdf_m_cost_at_boundary() {
    let app = TestAppBuilder::with_mocks().build();

    let (json_base, _) = valid_signup_with_keys("placeholder");
    let mut body: serde_json::Value = serde_json::from_str(&json_base).expect("json");
    // m_cost=65535 (one below 65536), valid t_cost and p_cost
    let raw = raw_envelope(0x01, 0x01, 65535, 3, 1);
    body["backup"]["encrypted_blob"] = serde_json::Value::String(encode_base64url(&raw));

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/auth/signup")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string()))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

/// Attack: Submit a signup with a whitespace-only device name ("   ").
///
/// Expected: 400 Bad Request — device names must be non-empty after trimming.
/// A whitespace-only name produces an invisible device in any UI display, making it
/// impossible for users to identify and manage the device.
#[shared_runtime_test]
async fn test_signup_device_name_whitespace_only() {
    let app = TestAppBuilder::with_mocks().build();

    let (json_base, _) = valid_signup_with_keys("placeholder");
    let mut body: serde_json::Value = serde_json::from_str(&json_base).expect("json");
    body["device"]["name"] = serde_json::Value::String("   ".to_string());

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/auth/signup")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string()))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

/// Attack: Submit a signup with a device name of 129 Unicode scalars (one over the
/// 128-scalar maximum).
///
/// Expected: 400 Bad Request — device name length is bounded by Unicode scalar count,
/// not byte count. Accepting over-long names could cause UI truncation anomalies or
/// storage capacity issues.
#[shared_runtime_test]
async fn test_signup_device_name_too_long() {
    let app = TestAppBuilder::with_mocks().build();

    let (json_base, _) = valid_signup_with_keys("placeholder");
    let mut body: serde_json::Value = serde_json::from_str(&json_base).expect("json");
    // 129 Unicode scalars, each ASCII 'a' counts as one scalar
    body["device"]["name"] = serde_json::Value::String("a".repeat(129));

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/auth/signup")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string()))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

/// Attack: Submit a signup with the `username` field set to JSON `null`.
///
/// Expected: 400 or 422 — `null` is not a valid string value. Accepting null as a
/// username would cause null-pointer-equivalent bugs in downstream string processing
/// or permit usernames that cannot be compared or displayed.
#[shared_runtime_test]
async fn test_signup_null_username() {
    let app = TestAppBuilder::with_mocks().build();

    let (json_base, _) = valid_signup_with_keys("placeholder");
    let mut body: serde_json::Value = serde_json::from_str(&json_base).expect("json");
    body["username"] = serde_json::Value::Null;

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/auth/signup")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string()))
                .expect("request"),
        )
        .await
        .expect("response");

    assert!(
        matches!(
            response.status(),
            StatusCode::BAD_REQUEST | StatusCode::UNPROCESSABLE_ENTITY
        ),
        "expected 400 or 422, got {}",
        response.status()
    );
}

/// Probe: Submit a valid signup body with an additional unknown field.
///
/// Expected: 201 Created — serde ignores unknown fields by default, so extra fields
/// must not cause rejection. This confirms the API will not break clients that send
/// additional fields for forward-compatibility purposes.
#[shared_runtime_test]
async fn test_signup_extra_unknown_fields_accepted() {
    let app = TestAppBuilder::with_mocks().build();

    let (json_base, _) = valid_signup_with_keys("adv_extra_fields");
    let mut body: serde_json::Value = serde_json::from_str(&json_base).expect("json");
    body["client_version"] = serde_json::Value::String("2.0.0".to_string());
    body["extra_hint"] = serde_json::Value::Bool(true);

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/auth/signup")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string()))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::CREATED);
}

// =========================================================================
// Section 3: Domain Logic Edge Cases
// =========================================================================

/// Attack: Register 10 device keys (the maximum) and then attempt to add an 11th.
///
/// Expected: 422 Unprocessable Entity — the server must enforce the maximum of 10
/// device keys per account. If accepted, an attacker with account access could add
/// unlimited devices, enabling persistent access after a bulk revocation attempt or
/// degrading system performance.
#[shared_runtime_test]
async fn test_eleventh_device_rejected() {
    let (app, keys, _db) = signup_user("adv_11th_device").await;

    // Signup creates device #1. Add devices #2 through #10.
    for i in 2..=10 {
        let new_key = SigningKey::generate(&mut OsRng);
        let new_pubkey = new_key.verifying_key().to_bytes();
        let cert = keys.root_signing_key.sign(&new_pubkey);

        let body = serde_json::json!({
            "pubkey": encode_base64url(&new_pubkey),
            "name": format!("Device {i}"),
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

        let resp = app.clone().oneshot(req).await.expect("response");
        assert_eq!(
            resp.status(),
            StatusCode::CREATED,
            "failed to add device #{i}"
        );
    }

    // Attempt to add device #11 — must be rejected
    let eleventh_key = SigningKey::generate(&mut OsRng);
    let eleventh_pubkey = eleventh_key.verifying_key().to_bytes();
    let cert = keys.root_signing_key.sign(&eleventh_pubkey);

    let body = serde_json::json!({
        "pubkey": encode_base64url(&eleventh_pubkey),
        "name": "Device 11 (over limit)",
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

    let resp = app.oneshot(req).await.expect("response");
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

/// Attack: Authenticate as Account A, then submit a POST /auth/devices request with a
/// device certificate signed by Account B's root key (not Account A's root key).
///
/// Expected: 400 Bad Request — the server must verify the certificate against the
/// authenticated account's root key, retrieved from the database. If this test fails,
/// Account B could issue device certificates that grant access to Account A, completely
/// breaking the account isolation model.
#[shared_runtime_test]
async fn test_cross_account_cert_on_add_device() {
    // Set up Account A with its own root key
    let (app, keys_a, db) = signup_user("adv_cross_add_a").await;

    // Set up Account B in the same database (needed to have a second root key)
    let (_, keys_b) = signup_user_in_pool("adv_cross_add_b", db.pool()).await;

    // Generate a new device key
    let new_device_key = SigningKey::generate(&mut OsRng);
    let new_device_pubkey = new_device_key.verifying_key().to_bytes();

    // Sign the certificate with Account B's root key instead of Account A's
    let cert_from_b = keys_b.root_signing_key.sign(&new_device_pubkey);

    let body = serde_json::json!({
        "pubkey": encode_base64url(&new_device_pubkey),
        "name": "Cross Account Cert Device",
        "certificate": encode_base64url(&cert_from_b.to_bytes()),
    })
    .to_string();

    // Authenticate as Account A but supply cert from Account B's root
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

/// Attack: Revoke a device, then attempt to revoke the same device again.
///
/// Expected: 409 Conflict — double-revocation must be rejected. Silently accepting
/// a second revocation could mask state machine bugs (e.g. a device appearing active
/// in a cache after a "second revocation" inadvertently clears its revoked status).
#[shared_runtime_test]
async fn test_double_revoke_returns_conflict() {
    let (app, keys, _db) = signup_user("adv_double_revoke").await;

    // Add a second device to use as the revocation target
    let second_key = SigningKey::generate(&mut OsRng);
    let second_pubkey = second_key.verifying_key().to_bytes();
    let cert = keys.root_signing_key.sign(&second_pubkey);
    let second_kid = Kid::derive(&second_pubkey);

    let add_body = serde_json::json!({
        "pubkey": encode_base64url(&second_pubkey),
        "name": "Double Revoke Target",
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
    let add_resp = app.clone().oneshot(add_req).await.expect("response");
    assert_eq!(add_resp.status(), StatusCode::CREATED);

    // First revocation — must succeed
    let revoke_path = format!("/auth/devices/{second_kid}");
    let revoke_req1 = build_authed_request(
        Method::DELETE,
        &revoke_path,
        "",
        &keys.device_signing_key,
        &keys.device_kid,
    );
    let resp1 = app.clone().oneshot(revoke_req1).await.expect("response");
    assert_eq!(resp1.status(), StatusCode::NO_CONTENT);

    // Second revocation of the same device — must return 409
    let revoke_req2 = build_authed_request(
        Method::DELETE,
        &revoke_path,
        "",
        &keys.device_signing_key,
        &keys.device_kid,
    );
    let resp2 = app.oneshot(revoke_req2).await.expect("response");
    assert_eq!(resp2.status(), StatusCode::CONFLICT);
}

/// Attack: Attempt to revoke a device KID that does not exist on the authenticated account.
///
/// Expected: 404 Not Found — the server must not reveal whether the KID exists on
/// another account. Returning 404 for all unknown KIDs prevents enumeration of other
/// accounts' device identifiers via cross-account probing.
#[shared_runtime_test]
async fn test_revoke_nonexistent_device() {
    let (app, keys, _db) = signup_user("adv_revoke_ghost").await;

    // Use a KID derived from a key pair that was never registered
    let phantom_key = SigningKey::generate(&mut OsRng);
    let phantom_kid = Kid::derive(&phantom_key.verifying_key().to_bytes());

    let path = format!("/auth/devices/{phantom_kid}");
    let req = build_authed_request(
        Method::DELETE,
        &path,
        "",
        &keys.device_signing_key,
        &keys.device_kid,
    );

    let response = app.oneshot(req).await.expect("response");
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

/// Attack: Revoke a device, then attempt to rename it.
///
/// Expected: 409 Conflict — a revoked device is logically decommissioned and must not
/// be modifiable. Allowing renames of revoked devices could allow an attacker to
/// retroactively alter device audit history to obscure malicious activity.
#[shared_runtime_test]
async fn test_rename_revoked_device_returns_conflict() {
    let (app, keys, _db) = signup_user("adv_rename_revoked").await;

    // Add a second device to revoke then attempt to rename
    let second_key = SigningKey::generate(&mut OsRng);
    let second_pubkey = second_key.verifying_key().to_bytes();
    let cert = keys.root_signing_key.sign(&second_pubkey);
    let second_kid = Kid::derive(&second_pubkey);

    let add_body = serde_json::json!({
        "pubkey": encode_base64url(&second_pubkey),
        "name": "About to be Revoked",
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
    let add_resp = app.clone().oneshot(add_req).await.expect("response");
    assert_eq!(add_resp.status(), StatusCode::CREATED);

    // Revoke the second device
    let revoke_path = format!("/auth/devices/{second_kid}");
    let revoke_req = build_authed_request(
        Method::DELETE,
        &revoke_path,
        "",
        &keys.device_signing_key,
        &keys.device_kid,
    );
    let revoke_resp = app.clone().oneshot(revoke_req).await.expect("response");
    assert_eq!(revoke_resp.status(), StatusCode::NO_CONTENT);

    // Attempt to rename the now-revoked device — must be rejected
    let rename_path = format!("/auth/devices/{second_kid}");
    let rename_body = serde_json::json!({ "name": "Renamed After Revoke" }).to_string();
    let rename_req = build_authed_request(
        Method::PATCH,
        &rename_path,
        &rename_body,
        &keys.device_signing_key,
        &keys.device_kid,
    );
    let rename_resp = app.oneshot(rename_req).await.expect("response");
    assert_eq!(rename_resp.status(), StatusCode::CONFLICT);
}

/// Attack: Attempt to rename a device KID that does not exist on the authenticated account.
///
/// Expected: 404 Not Found — same rationale as `test_revoke_nonexistent_device`.
/// Returning 404 for all unknown KIDs prevents enumeration of device identifiers
/// belonging to other accounts.
#[shared_runtime_test]
async fn test_rename_nonexistent_device() {
    let (app, keys, _db) = signup_user("adv_rename_ghost").await;

    let phantom_key = SigningKey::generate(&mut OsRng);
    let phantom_kid = Kid::derive(&phantom_key.verifying_key().to_bytes());

    let path = format!("/auth/devices/{phantom_kid}");
    let body = serde_json::json!({ "name": "Ghost Device Rename" }).to_string();

    let req = build_authed_request(
        Method::PATCH,
        &path,
        &body,
        &keys.device_signing_key,
        &keys.device_kid,
    );

    let response = app.oneshot(req).await.expect("response");
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
