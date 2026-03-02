//! Device authentication helpers for integration tests.
//!
//! Builds signed requests using the Ed25519 device key auth protocol
//! (X-Device-Kid, X-Signature, X-Timestamp, X-Nonce headers).

use axum::{
    body::Body,
    http::{header::CONTENT_TYPE, Method, Request},
};
use ed25519_dalek::{Signer, SigningKey};
use sha2::{Digest, Sha256};
use tc_crypto::{encode_base64url, Kid};

/// Build the auth headers for a device-authenticated request.
///
/// Returns header name/value pairs for X-Device-Kid, X-Signature,
/// X-Timestamp, and X-Nonce based on the canonical message format:
/// `{METHOD}\n{PATH}\n{TIMESTAMP}\n{NONCE}\n{BODY_SHA256_HEX}`
pub fn sign_request(
    method: &str,
    path: &str,
    body: &[u8],
    signing_key: &SigningKey,
    kid: &Kid,
) -> Vec<(&'static str, String)> {
    let timestamp = chrono::Utc::now().timestamp();
    let nonce = uuid::Uuid::new_v4().to_string();
    sign_request_at_timestamp(method, path, body, signing_key, kid, timestamp, &nonce)
}

/// Build auth headers for a device-authenticated request at a specific timestamp.
///
/// Like [`sign_request`], but accepts an explicit Unix timestamp and nonce
/// instead of using `Utc::now()` and a random UUID. This is useful for
/// testing timestamp skew enforcement and replay detection.
pub fn sign_request_at_timestamp(
    method: &str,
    path: &str,
    body: &[u8],
    signing_key: &SigningKey,
    kid: &Kid,
    timestamp: i64,
    nonce: &str,
) -> Vec<(&'static str, String)> {
    let body_hash = Sha256::digest(body);
    let body_hash_hex = format!("{body_hash:x}");
    let canonical = format!("{method}\n{path}\n{timestamp}\n{nonce}\n{body_hash_hex}");
    let signature = signing_key.sign(canonical.as_bytes());

    vec![
        ("X-Device-Kid", kid.to_string()),
        ("X-Signature", encode_base64url(&signature.to_bytes())),
        ("X-Timestamp", timestamp.to_string()),
        ("X-Nonce", nonce.to_string()),
    ]
}

/// Build a complete authenticated request for a device endpoint.
///
/// Wraps [`sign_request`] into a full `Request<Body>` with auth headers
/// and optional JSON content type.
pub fn build_authed_request(
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
