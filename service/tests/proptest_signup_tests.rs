//! Property-based tests for the signup endpoint.
//!
//! Uses `proptest` to generate thousands of near-valid signup payloads with
//! individual fields fuzzed. The invariant under test: **the server must never
//! return 500 for any combination of inputs**. A 500 means the handler panicked
//! or hit an unhandled error path — both are bugs.
//!
//! These tests complement the hand-written adversarial tests in
//! `adversarial_tests.rs` by exploring input space that humans wouldn't think of
//! (truncated UTF-8, boundary-length strings, exotic byte sequences, etc.).
//!
//! All tests use `TestAppBuilder::with_mocks()` — no real database is needed
//! because validation rejects malformed input before any persistence call.
//!
//! Run with: `cargo test --test proptest_signup_tests`

mod common;

use std::sync::OnceLock;

use axum::{
    body::Body,
    http::{header::CONTENT_TYPE, Method, Request, StatusCode},
};
use common::app_builder::TestAppBuilder;
use ed25519_dalek::{Signer, SigningKey};
use proptest::prelude::*;
use rand::rngs::OsRng;
use tc_crypto::{encode_base64url, BackupEnvelope};
use tower::ServiceExt;

fn shared_runtime() -> &'static tokio::runtime::Runtime {
    static RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RT.get_or_init(|| tokio::runtime::Runtime::new().expect("tokio runtime"))
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Build a valid signup JSON body and return it with the root/device keys.
///
/// Each field is independently replaceable so proptest strategies can swap
/// exactly one field while keeping the rest valid.
struct ValidSignupParts {
    username: String,
    root_pubkey: String,
    backup_blob: String,
    device_pubkey: String,
    device_name: String,
    certificate: String,
}

impl ValidSignupParts {
    fn generate() -> Self {
        let root_signing_key = SigningKey::generate(&mut OsRng);
        let root_pubkey_bytes = root_signing_key.verifying_key().to_bytes();

        let device_signing_key = SigningKey::generate(&mut OsRng);
        let device_pubkey_bytes = device_signing_key.verifying_key().to_bytes();

        let certificate_sig = root_signing_key.sign(&device_pubkey_bytes);

        let envelope = BackupEnvelope::build([0xAA; 16], 65536, 3, 1, [0xBB; 12], &[0xCC; 48])
            .expect("test envelope");

        Self {
            username: "proptestuser".to_string(),
            root_pubkey: encode_base64url(&root_pubkey_bytes),
            backup_blob: encode_base64url(envelope.as_bytes()),
            device_pubkey: encode_base64url(&device_pubkey_bytes),
            device_name: "Test Device".to_string(),
            certificate: encode_base64url(&certificate_sig.to_bytes()),
        }
    }

    fn to_json(&self) -> String {
        serde_json::json!({
            "username": self.username,
            "root_pubkey": self.root_pubkey,
            "backup": {"encrypted_blob": self.backup_blob},
            "device": {
                "pubkey": self.device_pubkey,
                "name": self.device_name,
                "certificate": self.certificate
            }
        })
        .to_string()
    }
}

/// Submit a JSON body to POST /auth/signup and return the status code.
fn submit_signup(json: &str) -> StatusCode {
    shared_runtime().block_on(async {
        let app = TestAppBuilder::with_mocks().build();
        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/auth/signup")
                    .header(CONTENT_TYPE, "application/json")
                    .body(Body::from(json.to_string()))
                    .expect("request"),
            )
            .await
            .expect("response");
        response.status()
    })
}

/// Submit a raw body (not necessarily valid JSON) to POST /auth/signup.
fn submit_signup_raw(body: &[u8]) -> StatusCode {
    let body_vec = body.to_vec();
    shared_runtime().block_on(async {
        let app = TestAppBuilder::with_mocks().build();
        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/auth/signup")
                    .header(CONTENT_TYPE, "application/json")
                    .body(Body::from(body_vec))
                    .expect("request"),
            )
            .await
            .expect("response");
        response.status()
    })
}

/// Assert that a status code is not 500 (Internal Server Error).
///
/// The server may legitimately return 400 (validation error), 422 (JSON
/// deserialization failure from Axum), or 201 (valid payload). It must
/// never return 500 from fuzzed input — that indicates a panic or
/// unhandled error in the validation path.
fn assert_no_server_error(status: StatusCode) {
    assert!(
        status != StatusCode::INTERNAL_SERVER_ERROR,
        "Server returned 500 Internal Server Error — this means the handler panicked \
         or hit an unhandled error path on fuzzed input. Status: {status}"
    );
}

// ─── Username fuzzing ────────────────────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    /// Fuzz the username with arbitrary Unicode strings (0–200 chars).
    ///
    /// Valid usernames are 3–64 ASCII alphanumeric + `_` + `-`, not reserved.
    /// Everything else should be rejected with 400, never 500.
    #[test]
    fn signup_never_500_on_random_username(username in "\\PC{0,200}") {
        let mut parts = ValidSignupParts::generate();
        parts.username = username;
        let status = submit_signup(&parts.to_json());
        assert_no_server_error(status);
    }

    /// Fuzz the username with strings near the length boundaries.
    ///
    /// Tests lengths 0, 1, 2, 3, 64, 65 — the validation boundaries.
    #[test]
    fn signup_username_boundary_lengths(len in 0usize..=130) {
        let mut parts = ValidSignupParts::generate();
        // Use 'a' repeated to hit length boundaries without triggering char validation
        parts.username = "a".repeat(len);
        let status = submit_signup(&parts.to_json());
        assert_no_server_error(status);

        // Short usernames (< 3) should be 400
        if len < 3 {
            prop_assert!(
                status == StatusCode::BAD_REQUEST,
                "Username of length {len} should be rejected, got {status}"
            );
        }
        // Long usernames (> 64) should be 400
        if len > 64 {
            prop_assert!(
                status == StatusCode::BAD_REQUEST,
                "Username of length {len} should be rejected, got {status}"
            );
        }
    }

    /// Test that reserved usernames are rejected regardless of casing.
    #[test]
    fn signup_reserved_usernames_rejected(
        word in prop::sample::select(vec![
            "admin", "administrator", "root", "system", "mod",
            "moderator", "support", "help", "api", "graphql",
            "auth", "signup", "login", "null", "undefined", "anonymous",
        ]),
        upper_mask in prop::collection::vec(any::<bool>(), 3..=13),
    ) {
        let mut parts = ValidSignupParts::generate();
        // Apply random casing to the reserved word
        let mixed_case: String = word
            .chars()
            .enumerate()
            .map(|(i, c)| {
                if upper_mask.get(i).copied().unwrap_or(false) {
                    c.to_ascii_uppercase()
                } else {
                    c
                }
            })
            .collect();
        parts.username = mixed_case;
        let status = submit_signup(&parts.to_json());
        assert_no_server_error(status);
        // Reserved usernames that are >= 3 chars should be 400
        if parts.username.len() >= 3 {
            prop_assert!(
                status == StatusCode::BAD_REQUEST,
                "Reserved username '{}' should be rejected, got {status}",
                parts.username
            );
        }
    }
}

// ─── Root pubkey fuzzing ─────────────────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    /// Fuzz root_pubkey with random byte arrays of various sizes.
    ///
    /// A valid Ed25519 public key is exactly 32 bytes (base64url-encoded).
    /// Other sizes should be rejected with 400, never 500.
    #[test]
    fn signup_never_500_on_random_root_pubkey(bytes in prop::collection::vec(any::<u8>(), 0..64)) {
        let mut parts = ValidSignupParts::generate();
        parts.root_pubkey = encode_base64url(&bytes);
        let status = submit_signup(&parts.to_json());
        assert_no_server_error(status);

        if bytes.len() == 32 {
            // Random 32 bytes are almost certainly not a valid Ed25519 curve point
            prop_assert!(
                status != StatusCode::INTERNAL_SERVER_ERROR,
                "server panicked on 32-byte non-curve-point root pubkey"
            );
        } else {
            prop_assert!(
                status == StatusCode::BAD_REQUEST,
                "root_pubkey of {} bytes should be rejected, got {status}",
                bytes.len()
            );
        }
    }

    /// Fuzz root_pubkey with arbitrary strings (not necessarily valid base64url).
    #[test]
    fn signup_never_500_on_malformed_root_pubkey(s in "\\PC{0,100}") {
        let mut parts = ValidSignupParts::generate();
        parts.root_pubkey = s;
        let status = submit_signup(&parts.to_json());
        assert_no_server_error(status);
    }
}

// ─── Device pubkey fuzzing ───────────────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    /// Fuzz device pubkey with random byte arrays (0–64 bytes).
    ///
    /// Must be exactly 32 bytes; other sizes should be rejected.
    #[test]
    fn signup_never_500_on_random_device_pubkey(bytes in prop::collection::vec(any::<u8>(), 0..64)) {
        let mut parts = ValidSignupParts::generate();
        parts.device_pubkey = encode_base64url(&bytes);
        let status = submit_signup(&parts.to_json());
        assert_no_server_error(status);

        if bytes.len() == 32 {
            // Random 32 bytes are almost certainly not a valid Ed25519 curve point
            prop_assert!(
                status != StatusCode::INTERNAL_SERVER_ERROR,
                "server panicked on 32-byte non-curve-point device pubkey"
            );
        } else {
            prop_assert!(
                status == StatusCode::BAD_REQUEST,
                "device pubkey of {} bytes should be rejected, got {status}",
                bytes.len()
            );
        }
    }
}

// ─── Certificate fuzzing ─────────────────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    /// Fuzz the certificate with random byte arrays.
    ///
    /// A valid certificate is exactly 64 bytes (Ed25519 signature). Wrong sizes
    /// should be rejected; 64-byte arrays with invalid signatures should also
    /// be rejected — but never with 500.
    #[test]
    fn signup_never_500_on_random_certificate(bytes in prop::collection::vec(any::<u8>(), 0..128)) {
        let mut parts = ValidSignupParts::generate();
        parts.certificate = encode_base64url(&bytes);
        let status = submit_signup(&parts.to_json());
        assert_no_server_error(status);

        // Wrong-size certificates must be rejected
        if bytes.len() != 64 {
            prop_assert!(
                status == StatusCode::BAD_REQUEST,
                "certificate of {} bytes should be rejected, got {status}",
                bytes.len()
            );
        }
    }

    /// Fuzz the certificate with arbitrary strings (not valid base64url).
    #[test]
    fn signup_never_500_on_malformed_certificate(s in "\\PC{0,200}") {
        let mut parts = ValidSignupParts::generate();
        parts.certificate = s;
        let status = submit_signup(&parts.to_json());
        assert_no_server_error(status);
    }
}

// ─── Backup envelope fuzzing ─────────────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    /// Fuzz the backup envelope with random bytes of various sizes.
    ///
    /// A valid envelope is 90–4096 bytes with specific header fields.
    /// Anything else should be rejected, never 500.
    #[test]
    fn signup_never_500_on_random_backup_blob(bytes in prop::collection::vec(any::<u8>(), 0..256)) {
        let mut parts = ValidSignupParts::generate();
        parts.backup_blob = encode_base64url(&bytes);
        let status = submit_signup(&parts.to_json());
        assert_no_server_error(status);
    }

    /// Fuzz the backup envelope with strings that aren't valid base64url.
    #[test]
    fn signup_never_500_on_malformed_backup_blob(s in "\\PC{0,200}") {
        let mut parts = ValidSignupParts::generate();
        parts.backup_blob = s;
        let status = submit_signup(&parts.to_json());
        assert_no_server_error(status);
    }

    /// Fuzz individual envelope header bytes.
    ///
    /// Starts with a valid envelope and corrupts a specific byte, testing
    /// that version/KDF/KDF-params validation catches every variant.
    #[test]
    fn signup_never_500_on_corrupted_envelope_header(
        byte_idx in 0usize..42,
        replacement in any::<u8>(),
    ) {
        let envelope = BackupEnvelope::build(
            [0xAA; 16], 65536, 3, 1, [0xBB; 12], &[0xCC; 48],
        )
        .expect("test envelope");
        let mut raw = envelope.as_bytes().to_vec();
        raw[byte_idx] = replacement;
        let mut parts = ValidSignupParts::generate();
        parts.backup_blob = encode_base64url(&raw);
        let status = submit_signup(&parts.to_json());
        assert_no_server_error(status);
    }
}

// ─── Device name fuzzing ─────────────────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    /// Fuzz the device name with arbitrary Unicode strings.
    ///
    /// Valid names: non-empty after trimming, at most 128 Unicode scalars.
    /// Empty or whitespace-only names should be 400.
    #[test]
    fn signup_never_500_on_random_device_name(name in "\\PC{0,300}") {
        let mut parts = ValidSignupParts::generate();
        parts.device_name = name.clone();
        let status = submit_signup(&parts.to_json());
        assert_no_server_error(status);

        // Empty or whitespace-only names must be rejected
        if name.trim().is_empty() {
            prop_assert!(
                status == StatusCode::BAD_REQUEST,
                "Empty/whitespace device name should be rejected, got {status}"
            );
        }
        // Names > 128 chars (after trim) must be rejected
        if name.trim().chars().count() > 128 {
            prop_assert!(
                status == StatusCode::BAD_REQUEST,
                "Device name with {} chars should be rejected, got {status}",
                name.trim().chars().count()
            );
        }
    }

    /// Test device names at length boundaries.
    #[test]
    fn signup_device_name_boundary_lengths(len in 0usize..=200) {
        let mut parts = ValidSignupParts::generate();
        // Use 'x' repeated to hit length boundaries without whitespace trimming issues
        parts.device_name = "x".repeat(len);
        let status = submit_signup(&parts.to_json());
        assert_no_server_error(status);

        if len == 0 {
            prop_assert!(
                status == StatusCode::BAD_REQUEST,
                "Empty device name should be rejected, got {status}"
            );
        }
        if len > 128 {
            prop_assert!(
                status == StatusCode::BAD_REQUEST,
                "Device name of length {len} should be rejected, got {status}"
            );
        }
    }
}

// ─── Whole-body fuzzing ──────────────────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    /// Submit completely random bytes as the request body.
    ///
    /// Axum's JSON extractor should reject invalid JSON with 422.
    /// Must never return 500.
    #[test]
    fn signup_never_500_on_random_bytes(bytes in prop::collection::vec(any::<u8>(), 0..1024)) {
        let status = submit_signup_raw(&bytes);
        assert_no_server_error(status);
    }

    /// Submit random strings (not necessarily valid JSON) as the request body.
    #[test]
    fn signup_never_500_on_random_string_body(s in "\\PC{0,500}") {
        let status = submit_signup_raw(s.as_bytes());
        assert_no_server_error(status);
    }

    /// Submit JSON objects with missing required fields.
    ///
    /// The server should reject incomplete payloads with 422 (missing fields
    /// fail JSON deserialization) or 400, never 500.
    #[test]
    fn signup_never_500_on_partial_json(
        has_username in any::<bool>(),
        has_root_pubkey in any::<bool>(),
        has_backup in any::<bool>(),
        has_device in any::<bool>(),
    ) {
        let parts = ValidSignupParts::generate();
        let mut obj = serde_json::Map::new();

        if has_username {
            obj.insert("username".to_string(), serde_json::json!(parts.username));
        }
        if has_root_pubkey {
            obj.insert("root_pubkey".to_string(), serde_json::json!(parts.root_pubkey));
        }
        if has_backup {
            obj.insert("backup".to_string(), serde_json::json!({"encrypted_blob": parts.backup_blob}));
        }
        if has_device {
            obj.insert("device".to_string(), serde_json::json!({
                "pubkey": parts.device_pubkey,
                "name": parts.device_name,
                "certificate": parts.certificate,
            }));
        }

        let json = serde_json::Value::Object(obj).to_string();
        let status = submit_signup(&json);
        assert_no_server_error(status);

        // If any required field is missing, expect rejection
        if !(has_username && has_root_pubkey && has_backup && has_device) {
            prop_assert!(
                status == StatusCode::BAD_REQUEST || status == StatusCode::UNPROCESSABLE_ENTITY,
                "Missing fields should be rejected, got {status}"
            );
        }
    }

    /// Submit JSON with unexpected types for each field.
    ///
    /// e.g., username as a number, root_pubkey as an array, etc.
    #[test]
    fn signup_never_500_on_wrong_json_types(
        username_type in 0u8..4,
        root_pubkey_type in 0u8..4,
    ) {
        let username_val = match username_type {
            0 => serde_json::json!("valid_user"),
            1 => serde_json::json!(42),
            2 => serde_json::json!(null),
            _ => serde_json::json!(["array"]),
        };
        let root_pubkey_val = match root_pubkey_type {
            0 => serde_json::json!("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="),
            1 => serde_json::json!(12345),
            2 => serde_json::json!(null),
            _ => serde_json::json!({"nested": "object"}),
        };

        let json = serde_json::json!({
            "username": username_val,
            "root_pubkey": root_pubkey_val,
            "backup": {"encrypted_blob": "dGVzdA"},
            "device": {
                "pubkey": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
                "name": "Test",
                "certificate": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"
            }
        })
        .to_string();

        let status = submit_signup(&json);
        assert_no_server_error(status);
    }
}

// ─── Multi-field fuzzing ─────────────────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    /// Fuzz multiple fields simultaneously.
    ///
    /// This catches interactions between validation steps — e.g., an invalid
    /// username combined with an invalid pubkey should not trigger a different
    /// code path than either alone.
    #[test]
    fn signup_never_500_on_multi_field_fuzz(
        username in "\\PC{0,100}",
        root_pubkey_bytes in prop::collection::vec(any::<u8>(), 0..64),
        device_pubkey_bytes in prop::collection::vec(any::<u8>(), 0..64),
        backup_bytes in prop::collection::vec(any::<u8>(), 0..256),
        device_name in "\\PC{0,200}",
        cert_bytes in prop::collection::vec(any::<u8>(), 0..128),
    ) {
        let json = serde_json::json!({
            "username": username,
            "root_pubkey": encode_base64url(&root_pubkey_bytes),
            "backup": {"encrypted_blob": encode_base64url(&backup_bytes)},
            "device": {
                "pubkey": encode_base64url(&device_pubkey_bytes),
                "name": device_name,
                "certificate": encode_base64url(&cert_bytes),
            }
        })
        .to_string();

        let status = submit_signup(&json);
        assert_no_server_error(status);
    }
}
