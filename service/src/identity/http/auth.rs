//! Request authentication via signed headers
//!
//! Device endpoints authenticate requests by verifying an Ed25519 signature
//! over a canonical message built from request parts.
//!
//! Canonical message format:
//! ```text
//! {METHOD}\n{PATH}\n{TIMESTAMP}\n{BODY_SHA256_HEX}
//! ```
//!
//! Required headers:
//! - `X-Device-Kid`: 22-char base64url key identifier
//! - `X-Signature`: base64url Ed25519 signature of the canonical message
//! - `X-Timestamp`: Unix seconds
//!
//! Replay protection records each signature's hash in the database, so requests
//! are **non-idempotent under network retries**. If the server records the nonce
//! but the response is lost, a client retry with the same signed payload will be
//! rejected. Clients must generate a fresh signature (with a current timestamp)
//! for every retry attempt.

use std::sync::Arc;

use axum::{
    body::Bytes,
    extract::{FromRequest, Request},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::ErrorResponse;
use crate::identity::repo::{DeviceKeyRepoError, IdentityRepo, NonceError};
use tc_crypto::{decode_base64url, verify_ed25519, Kid};

/// Maximum clock skew allowed for timestamps (seconds).
///
/// Also used as the nonce TTL — expired nonces are cleaned up after this window.
pub const MAX_TIMESTAMP_SKEW: i64 = 300;

/// Maximum request body size for authenticated device endpoints (64 KiB).
///
/// Device management payloads (JSON with keys, names, certificates) are small;
/// 64 KiB is generous. A tighter limit prevents abuse of the body-read step
/// before signature verification.
const MAX_BODY_SIZE: usize = 64 * 1024;

/// Authenticated device extracted from signed request headers.
///
/// Implements `FromRequest` — reads the full body, verifies the signature,
/// and makes the raw body available via `json()` for handlers that need it.
pub struct AuthenticatedDevice {
    pub account_id: Uuid,
    pub device_kid: Kid,
    body_bytes: Bytes,
}

impl AuthenticatedDevice {
    /// Deserialize the request body as JSON.
    ///
    /// # Errors
    ///
    /// Returns a 400 response if the body is not valid JSON for `T`.
    #[allow(clippy::result_large_err)]
    pub fn json<T: serde::de::DeserializeOwned>(&self) -> Result<T, Response> {
        serde_json::from_slice(&self.body_bytes)
            .map_err(|e| super::bad_request(&format!("Invalid JSON body: {e}")))
    }
}

fn auth_error(msg: &str) -> Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(ErrorResponse {
            error: msg.to_string(),
        }),
    )
        .into_response()
}

fn forbidden_error(msg: &str) -> Response {
    (
        StatusCode::FORBIDDEN,
        Json(ErrorResponse {
            error: msg.to_string(),
        }),
    )
        .into_response()
}

impl<S: Send + Sync> FromRequest<S> for AuthenticatedDevice {
    type Rejection = Response;

    async fn from_request(req: Request, _state: &S) -> Result<Self, Self::Rejection> {
        // Extract repo from extensions before consuming the request
        let repo = req
            .extensions()
            .get::<Arc<dyn IdentityRepo>>()
            .ok_or_else(|| auth_error("Server misconfiguration"))?
            .clone();

        // Extract headers
        let kid_str = req
            .headers()
            .get("X-Device-Kid")
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| auth_error("Missing X-Device-Kid header"))?
            .to_string();

        let signature_str = req
            .headers()
            .get("X-Signature")
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| auth_error("Missing X-Signature header"))?
            .to_string();

        let timestamp_str = req
            .headers()
            .get("X-Timestamp")
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| auth_error("Missing X-Timestamp header"))?
            .to_string();

        // Parse KID
        let kid: Kid = kid_str
            .parse()
            .map_err(|_| auth_error("Invalid KID format"))?;

        // Parse and validate timestamp
        let timestamp: i64 = timestamp_str
            .parse()
            .map_err(|_| auth_error("Invalid timestamp"))?;

        let now = chrono::Utc::now().timestamp();
        let skew = (now - timestamp).abs();
        if skew > MAX_TIMESTAMP_SKEW {
            return Err(auth_error("Timestamp out of range"));
        }

        // Decode signature
        let sig_bytes = decode_base64url(&signature_str)
            .map_err(|_| auth_error("Invalid signature encoding"))?;
        let sig_arr: [u8; 64] = sig_bytes
            .as_slice()
            .try_into()
            .map_err(|_| auth_error("Signature must be 64 bytes"))?;

        // Capture method and path+query before consuming the request.
        // Include query string in the signed payload so future endpoints
        // with query parameters are protected against parameter injection.
        let method = req.method().to_string();
        let path = req.uri().path_and_query().map_or_else(
            || req.uri().path().to_string(),
            |pq| pq.as_str().to_string(),
        );

        // Read the body
        let body_bytes = axum::body::to_bytes(req.into_body(), MAX_BODY_SIZE)
            .await
            .map_err(|_| auth_error("Failed to read request body"))?;

        // Compute body hash
        let body_hash = Sha256::digest(&body_bytes);
        let body_hash_hex = format!("{body_hash:x}");

        // Build canonical message
        let canonical = format!("{method}\n{path}\n{timestamp}\n{body_hash_hex}");

        // Look up device
        let device = repo
            .get_device_key_by_kid(&kid)
            .await
            .map_err(|e| match e {
                DeviceKeyRepoError::NotFound => auth_error("Device not found"),
                DeviceKeyRepoError::Database(db_err) => {
                    tracing::error!("Auth device lookup failed: {db_err}");
                    auth_error("Authentication failed")
                }
                DeviceKeyRepoError::DuplicateKid
                | DeviceKeyRepoError::AlreadyRevoked
                | DeviceKeyRepoError::MaxDevicesReached => {
                    tracing::error!("Unexpected repo error during auth lookup: {e}");
                    auth_error("Authentication failed")
                }
            })?;

        // Decode stored public key
        let pubkey_bytes = decode_base64url(&device.device_pubkey)
            .map_err(|_| auth_error("Corrupted device key"))?;
        let pubkey_arr: [u8; 32] = pubkey_bytes
            .as_slice()
            .try_into()
            .map_err(|_| auth_error("Corrupted device key"))?;

        // Verify signature BEFORE checking revocation status.
        // If we checked revocation first, an unauthenticated caller who knows
        // a valid KID could distinguish revoked (403) from active (401) devices
        // without possessing the private key.
        verify_ed25519(&pubkey_arr, canonical.as_bytes(), &sig_arr)
            .map_err(|_| auth_error("Invalid signature"))?;

        // Check if revoked (after signature verification to avoid status oracle).
        // Must happen before nonce recording so a revoked device's valid request
        // doesn't consume a nonce slot (which would leak "request was seen" via
        // a replay returning 401 instead of 403).
        if device.revoked_at.is_some() {
            return Err(forbidden_error("Device has been revoked"));
        }

        // Replay protection: record a hash of the signature to prevent reuse
        // within the timestamp window.
        check_nonce(&*repo, &sig_arr).await?;

        // Touch last_used_at (fire-and-forget, don't fail the request)
        let touch_kid = kid.clone();
        let touch_repo = repo;
        tokio::spawn(async move {
            if let Err(e) = touch_repo.touch_device_key(&touch_kid).await {
                tracing::warn!("Failed to touch device {touch_kid}: {e}");
            }
        });

        Ok(Self {
            account_id: device.account_id,
            device_kid: kid,
            body_bytes,
        })
    }
}

/// Record a signature nonce to prevent replay attacks.
async fn check_nonce(repo: &dyn IdentityRepo, sig_bytes: &[u8; 64]) -> Result<(), Response> {
    let nonce_hash: [u8; 32] = Sha256::digest(sig_bytes).into();
    repo.check_and_record_nonce(&nonce_hash)
        .await
        .map_err(|e| match e {
            NonceError::Replay => auth_error("Request replay detected"),
            NonceError::Database(db_err) => {
                tracing::error!("Nonce check failed: {db_err}");
                auth_error("Authentication failed")
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kid_parse_valid() {
        // 22-char base64url string
        let kid: Result<Kid, _> = "cs1uhCLEB_ttCYaQ8RMLfQ".parse();
        assert!(kid.is_ok());
    }

    #[test]
    fn test_kid_parse_invalid_length() {
        let kid: Result<Kid, _> = "tooshort".parse();
        assert!(kid.is_err());
    }

    #[test]
    fn test_timestamp_validation() {
        let now = chrono::Utc::now().timestamp();

        // Within range
        assert!((now - now).abs() <= MAX_TIMESTAMP_SKEW);

        // Outside range
        let old = now - MAX_TIMESTAMP_SKEW - 1;
        assert!((now - old).abs() > MAX_TIMESTAMP_SKEW);

        let future = now + MAX_TIMESTAMP_SKEW + 1;
        assert!((now - future).abs() > MAX_TIMESTAMP_SKEW);
    }

    #[test]
    fn test_canonical_message_format() {
        let method = "GET";
        let path = "/auth/devices";
        let timestamp = 1_700_000_000_i64;
        let body_hash_hex = format!("{:x}", Sha256::digest(b""));

        let canonical = format!("{method}\n{path}\n{timestamp}\n{body_hash_hex}");

        assert!(canonical.starts_with("GET\n/auth/devices\n1700000000\n"));
        // SHA-256 of empty body is well-known
        assert!(
            canonical.ends_with("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855")
        );
    }
}
