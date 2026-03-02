//! Snapshot-based adversarial tests — captures full HTTP response (status + body).
//!
//! These tests complement `adversarial_tests.rs` by recording the entire error
//! response as an insta snapshot. Regressions in error messages, error codes,
//! or response structure show up as snapshot diffs.
//!
//! All tests in this file use `TestAppBuilder::with_mocks()` so they run
//! without a database (no `tc-postgres:local` container required).
//!
//! Run with: `cargo test --test snapshot_adversarial_tests`

mod common;

use axum::{
    body::Body,
    http::{header::CONTENT_TYPE, Method, Request, StatusCode},
};
use common::app_builder::TestAppBuilder;
use ed25519_dalek::{Signer, SigningKey};
use rand::rngs::OsRng;
use sha2::{Digest, Sha256};
use tc_crypto::{encode_base64url, BackupEnvelope, Kid};
use tc_test_macros::shared_runtime_test;
use tower::ServiceExt;

// ── Snapshot helper ─────────────────────────────────────────────────────────

/// Extract status and body from a response, then snapshot both.
///
/// If the body is valid JSON, uses `assert_json_snapshot!` for structured diffs.
/// Otherwise falls back to a plain-text snapshot.
async fn snapshot_response(name: &str, response: axum::http::Response<Body>) {
    let status = response.status();
    let body_bytes = axum::body::to_bytes(response.into_body(), 4096)
        .await
        .expect("read body");

    if let Ok(mut json) = serde_json::from_slice::<serde_json::Value>(&body_bytes) {
        // Inject status into the snapshot so it's tracked alongside the body
        if let Some(obj) = json.as_object_mut() {
            obj.insert(
                "_status".to_string(),
                serde_json::Value::Number(status.as_u16().into()),
            );
        }
        insta::assert_json_snapshot!(name, json, {
            ".timestamp" => "[timestamp]",
            ".request_id" => "[request_id]",
        });
    } else {
        let text = String::from_utf8_lossy(&body_bytes);
        let combined = format!("status: {}\nbody: {text}", status.as_u16());
        insta::assert_snapshot!(name, combined);
    }
}

// =========================================================================
// Trust Boundary: Signup — forged certificate
// =========================================================================

/// Snapshot the full error response when a signup request includes a device
/// certificate signed by a random key (not the account's root key).
#[shared_runtime_test]
async fn test_snapshot_forged_certificate_response() {
    let app = TestAppBuilder::with_mocks().build();

    let root_signing_key = SigningKey::generate(&mut OsRng);
    let root_pubkey_bytes = root_signing_key.verifying_key().to_bytes();
    let root_pubkey = encode_base64url(&root_pubkey_bytes);

    let device_signing_key = SigningKey::generate(&mut OsRng);
    let device_pubkey_bytes = device_signing_key.verifying_key().to_bytes();
    let device_pubkey = encode_base64url(&device_pubkey_bytes);

    // Forge: sign device pubkey with a random key, not the root key.
    let forger_key = SigningKey::generate(&mut OsRng);
    let forged_cert = forger_key.sign(&device_pubkey_bytes);
    let certificate = encode_base64url(&forged_cert.to_bytes());

    let envelope = BackupEnvelope::build([0xAA; 16], 65536, 3, 1, [0xBB; 12], &[0xCC; 48])
        .expect("test envelope");
    let backup_blob = encode_base64url(envelope.as_bytes());

    let json = serde_json::json!({
        "username": "snap_forged_cert",
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
    snapshot_response("forged_certificate_signup", response).await;
}

// =========================================================================
// Trust Boundary: Request — future timestamp outside allowed skew
// =========================================================================

/// Snapshot the full error response when a request carries a timestamp
/// 10 minutes in the future (well beyond the 300s skew window).
#[shared_runtime_test]
async fn test_snapshot_future_timestamp_response() {
    let app = TestAppBuilder::with_mocks().build();

    let signing_key = SigningKey::generate(&mut OsRng);
    let kid = Kid::derive(&signing_key.verifying_key().to_bytes());

    let future_timestamp = chrono::Utc::now().timestamp() + 600;
    let nonce = uuid::Uuid::new_v4().to_string();
    let body_hash = Sha256::digest(b"");
    let body_hash_hex = format!("{body_hash:x}");
    let canonical = format!("GET\n/auth/devices\n{future_timestamp}\n{nonce}\n{body_hash_hex}");
    let signature = signing_key.sign(canonical.as_bytes());

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/auth/devices")
                .header("X-Device-Kid", kid.to_string())
                .header("X-Signature", encode_base64url(&signature.to_bytes()))
                .header("X-Timestamp", future_timestamp.to_string())
                .header("X-Nonce", &nonce)
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    snapshot_response("future_timestamp_rejected", response).await;
}

// =========================================================================
// Trust Boundary: Request — empty X-Signature header
// =========================================================================

/// Snapshot the full error response when the X-Signature header is present
/// but contains an empty string (decodes to 0 bytes, not 64).
#[shared_runtime_test]
async fn test_snapshot_empty_signature_response() {
    let app = TestAppBuilder::with_mocks().build();

    let kid = Kid::derive(&[0u8; 32]);
    let timestamp = chrono::Utc::now().timestamp();
    let nonce = uuid::Uuid::new_v4().to_string();

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/auth/devices")
                .header("X-Device-Kid", kid.to_string())
                .header("X-Signature", "")
                .header("X-Timestamp", timestamp.to_string())
                .header("X-Nonce", &nonce)
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    snapshot_response("empty_signature_rejected", response).await;
}

// =========================================================================
// Trust Boundary: Signup — malformed JSON body
// =========================================================================

/// Snapshot the full error response when the signup endpoint receives
/// syntactically invalid JSON.
#[shared_runtime_test]
async fn test_snapshot_malformed_json_body_response() {
    let app = TestAppBuilder::with_mocks().build();

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/auth/signup")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from("{not valid json"))
                .expect("request"),
        )
        .await
        .expect("response");

    // Axum returns 400 for JSON parse failures on this endpoint
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    snapshot_response("malformed_json_signup", response).await;
}

// =========================================================================
// Trust Boundary: Request — missing all auth headers
// =========================================================================

/// Snapshot the full error response when a request to an authenticated
/// endpoint is sent with no auth headers at all.
#[shared_runtime_test]
async fn test_snapshot_missing_auth_headers_response() {
    let app = TestAppBuilder::with_mocks().build();

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/auth/devices")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    snapshot_response("missing_auth_headers", response).await;
}

// =========================================================================
// Trust Boundary: Request — invalid KID format
// =========================================================================

/// Snapshot the full error response when X-Device-Kid contains a value
/// that is not a valid 22-character base64url KID.
#[shared_runtime_test]
async fn test_snapshot_invalid_kid_format_response() {
    let app = TestAppBuilder::with_mocks().build();

    let timestamp = chrono::Utc::now().timestamp();
    let nonce = uuid::Uuid::new_v4().to_string();

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/auth/devices")
                .header("X-Device-Kid", "too-short")
                .header("X-Signature", encode_base64url(&[0u8; 64]))
                .header("X-Timestamp", timestamp.to_string())
                .header("X-Nonce", &nonce)
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    snapshot_response("invalid_kid_format", response).await;
}

// =========================================================================
// Trust Boundary: Signup — missing required fields
// =========================================================================

/// Snapshot the full error response when the signup body is valid JSON
/// but is missing required fields.
#[shared_runtime_test]
async fn test_snapshot_incomplete_signup_body_response() {
    let app = TestAppBuilder::with_mocks().build();

    let json = serde_json::json!({
        "username": "snap_incomplete"
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

    // Axum returns 422 when required fields are missing from JSON
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    snapshot_response("incomplete_signup_body", response).await;
}

// =========================================================================
// Trust Boundary: Request — invalid signature encoding
// =========================================================================

/// Snapshot the full error response when the X-Signature header contains
/// a value that is not valid base64url.
#[shared_runtime_test]
async fn test_snapshot_invalid_signature_encoding_response() {
    let app = TestAppBuilder::with_mocks().build();

    let kid = Kid::derive(&[0u8; 32]);
    let timestamp = chrono::Utc::now().timestamp();
    let nonce = uuid::Uuid::new_v4().to_string();

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/auth/devices")
                .header("X-Device-Kid", kid.to_string())
                .header("X-Signature", "!!!not-base64url!!!")
                .header("X-Timestamp", timestamp.to_string())
                .header("X-Nonce", &nonce)
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    snapshot_response("invalid_signature_encoding", response).await;
}

// =========================================================================
// Trust Boundary: Request — non-numeric timestamp
// =========================================================================

/// Snapshot the full error response when X-Timestamp contains a
/// non-numeric value.
#[shared_runtime_test]
async fn test_snapshot_non_numeric_timestamp_response() {
    let app = TestAppBuilder::with_mocks().build();

    let kid = Kid::derive(&[0u8; 32]);
    let nonce = uuid::Uuid::new_v4().to_string();

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/auth/devices")
                .header("X-Device-Kid", kid.to_string())
                .header("X-Signature", encode_base64url(&[0u8; 64]))
                .header("X-Timestamp", "not-a-number")
                .header("X-Nonce", &nonce)
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    snapshot_response("non_numeric_timestamp", response).await;
}
