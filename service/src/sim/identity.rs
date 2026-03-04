//! Deterministic identity for simulation accounts.
//!
//! Each [`SimAccount`] derives Ed25519 key pairs from a seed index so that
//! repeated runs produce identical cryptographic material. The struct can
//! build the JSON body for `POST /auth/signup` and sign authenticated
//! requests using the device-auth protocol.

use ed25519_dalek::{Signer, SigningKey};
use sha2::{Digest, Sha256};
use tc_crypto::{encode_base64url, BackupEnvelope, Kid};
use uuid::Uuid;

/// A deterministic simulation account with root and device key pairs.
///
/// Created via [`SimAccount::from_seed`]; the same index always yields
/// the same keys, username, and signup payload.
pub struct SimAccount {
    pub username: String,
    pub account_id: Option<Uuid>,
    root_signing_key: SigningKey,
    device_signing_key: SigningKey,
    pub device_kid: Kid,
}

impl SimAccount {
    /// Derive a simulation account from a numeric seed index.
    ///
    /// Root key: `SHA-256("tc-sim-root-key-v1-{index}")` -> first 32 bytes -> `SigningKey`.
    /// Device key: `SHA-256("tc-sim-device-key-v1-{index}")` -> first 32 bytes -> `SigningKey`.
    /// Username: `sim_voter_{index:02}`.
    #[must_use]
    pub fn from_seed(index: usize) -> Self {
        let root_seed: [u8; 32] = Sha256::digest(format!("tc-sim-root-key-v1-{index}")).into();
        let root_signing_key = SigningKey::from_bytes(&root_seed);

        let device_seed: [u8; 32] = Sha256::digest(format!("tc-sim-device-key-v1-{index}")).into();
        let device_signing_key = SigningKey::from_bytes(&device_seed);

        let device_pubkey_bytes = device_signing_key.verifying_key().to_bytes();
        let device_kid = Kid::derive(&device_pubkey_bytes);

        Self {
            username: format!("sim_voter_{index:02}"),
            account_id: None,
            root_signing_key,
            device_signing_key,
            device_kid,
        }
    }

    /// Build the JSON body for `POST /auth/signup`.
    ///
    /// The output matches the format expected by the signup endpoint:
    /// root pubkey, backup envelope, and device key with certificate.
    ///
    /// # Errors
    ///
    /// Returns `EnvelopeError` if the backup envelope cannot be constructed
    /// (should not happen with the fixed placeholder values).
    pub fn build_signup_json(&self) -> Result<String, tc_crypto::EnvelopeError> {
        let root_pubkey_bytes = self.root_signing_key.verifying_key().to_bytes();
        let root_pubkey = encode_base64url(&root_pubkey_bytes);

        let device_pubkey_bytes = self.device_signing_key.verifying_key().to_bytes();
        let device_pubkey = encode_base64url(&device_pubkey_bytes);

        // Root key signs the device public key bytes to produce the certificate.
        let certificate_sig = self.root_signing_key.sign(&device_pubkey_bytes);
        let certificate = encode_base64url(&certificate_sig.to_bytes());

        // Placeholder backup envelope with deterministic test values.
        let envelope = BackupEnvelope::build(
            [0xAA; 16], // salt
            65536,
            3,
            1,           // m_cost, t_cost, p_cost
            [0xBB; 12],  // nonce
            &[0xCC; 48], // ciphertext
        )?;
        let backup_blob = encode_base64url(envelope.as_bytes());

        let username = &self.username;
        Ok(format!(
            r#"{{"username": "{username}", "root_pubkey": "{root_pubkey}", "backup": {{"encrypted_blob": "{backup_blob}"}}, "device": {{"pubkey": "{device_pubkey}", "name": "Sim Device", "certificate": "{certificate}"}}}}"#
        ))
    }

    /// Create a deterministic verifier account.
    ///
    /// Uses a separate seed domain (`"tc-sim-verifier-*"`) so keys never
    /// collide with voter accounts created via [`SimAccount::from_seed`].
    /// The username is `"sim_verifier"`.
    #[must_use]
    pub fn verifier() -> Self {
        let root_seed: [u8; 32] = Sha256::digest(b"tc-sim-verifier-root-key-v1").into();
        let root_signing_key = SigningKey::from_bytes(&root_seed);

        let device_seed: [u8; 32] = Sha256::digest(b"tc-sim-verifier-device-key-v1").into();
        let device_signing_key = SigningKey::from_bytes(&device_seed);

        let device_pubkey_bytes = device_signing_key.verifying_key().to_bytes();
        let device_kid = Kid::derive(&device_pubkey_bytes);

        Self {
            username: "sim_verifier".to_string(),
            account_id: None,
            root_signing_key,
            device_signing_key,
            device_kid,
        }
    }

    /// Create a deterministic demo verifier account.
    ///
    /// Uses a separate seed domain (`"tc-demo-verifier-*"`) so keys never
    /// collide with voter accounts or the sim verifier. The username is
    /// `"demo_verifier"`.
    #[must_use]
    pub fn demo_verifier() -> Self {
        let root_seed: [u8; 32] = Sha256::digest(b"tc-demo-verifier-root-key-v1").into();
        let root_signing_key = SigningKey::from_bytes(&root_seed);

        let device_seed: [u8; 32] = Sha256::digest(b"tc-demo-verifier-device-key-v1").into();
        let device_signing_key = SigningKey::from_bytes(&device_seed);

        let device_pubkey_bytes = device_signing_key.verifying_key().to_bytes();
        let device_kid = Kid::derive(&device_pubkey_bytes);

        Self {
            username: "demo_verifier".to_string(),
            account_id: None,
            root_signing_key,
            device_signing_key,
            device_kid,
        }
    }

    /// Return the root public key as base64url for use in `TC_VERIFIERS` config.
    #[must_use]
    pub fn root_pubkey_base64url(&self) -> String {
        encode_base64url(&self.root_signing_key.verifying_key().to_bytes())
    }

    /// Build the JSON body for `POST /auth/login`.
    ///
    /// The login payload registers a new device key for an existing account.
    /// The certificate is `root_key.sign(device_pubkey || timestamp_le_i64_bytes)`.
    #[must_use]
    pub fn build_login_json(&self) -> String {
        let timestamp = chrono::Utc::now().timestamp();

        let device_pubkey_bytes = self.device_signing_key.verifying_key().to_bytes();
        let device_pubkey = encode_base64url(&device_pubkey_bytes);

        // Certificate: root signs (device_pubkey || timestamp as LE i64 bytes)
        let mut signed_payload = Vec::with_capacity(40);
        signed_payload.extend_from_slice(&device_pubkey_bytes);
        signed_payload.extend_from_slice(&timestamp.to_le_bytes());
        let cert = self.root_signing_key.sign(&signed_payload);
        let certificate = encode_base64url(&cert.to_bytes());

        format!(
            r#"{{"username": "{}", "timestamp": {timestamp}, "device": {{"pubkey": "{device_pubkey}", "name": "Sim Device", "certificate": "{certificate}"}}}}"#,
            self.username
        )
    }

    /// Produce device-auth headers for an authenticated API request.
    ///
    /// Signs the canonical message `{method}\n{path}\n{timestamp}\n{nonce}\n{body_sha256_hex}`
    /// using the **device** signing key. Returns the four headers expected by
    /// the auth middleware: `X-Device-Kid`, `X-Signature`, `X-Timestamp`, `X-Nonce`.
    #[must_use]
    pub fn sign_request(
        &self,
        method: &str,
        path: &str,
        body: &[u8],
    ) -> Vec<(&'static str, String)> {
        let timestamp = chrono::Utc::now().timestamp();
        let nonce = Uuid::new_v4().to_string();

        let body_hash = Sha256::digest(body);
        let body_hash_hex = format!("{body_hash:x}");
        let canonical = format!("{method}\n{path}\n{timestamp}\n{nonce}\n{body_hash_hex}");
        let signature = self.device_signing_key.sign(canonical.as_bytes());

        vec![
            ("X-Device-Kid", self.device_kid.to_string()),
            ("X-Signature", encode_base64url(&signature.to_bytes())),
            ("X-Timestamp", timestamp.to_string()),
            ("X-Nonce", nonce),
        ]
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn same_index_produces_same_keys() {
        let a = SimAccount::from_seed(0);
        let b = SimAccount::from_seed(0);

        assert_eq!(
            a.root_signing_key.to_bytes(),
            b.root_signing_key.to_bytes(),
            "root keys must be deterministic"
        );
        assert_eq!(
            a.device_signing_key.to_bytes(),
            b.device_signing_key.to_bytes(),
            "device keys must be deterministic"
        );
        assert_eq!(a.device_kid, b.device_kid);
        assert_eq!(a.username, b.username);
    }

    #[test]
    fn different_indices_produce_different_keys() {
        let a = SimAccount::from_seed(0);
        let b = SimAccount::from_seed(1);

        assert_ne!(
            a.root_signing_key.to_bytes(),
            b.root_signing_key.to_bytes(),
            "different indices must yield different root keys"
        );
        assert_ne!(
            a.device_signing_key.to_bytes(),
            b.device_signing_key.to_bytes(),
            "different indices must yield different device keys"
        );
        assert_ne!(a.device_kid, b.device_kid);
        assert_ne!(a.username, b.username);
    }

    #[test]
    fn root_and_device_keys_differ() {
        let account = SimAccount::from_seed(0);
        assert_ne!(
            account.root_signing_key.to_bytes(),
            account.device_signing_key.to_bytes(),
            "root and device keys for the same account must differ"
        );
    }

    #[test]
    fn username_format() {
        assert_eq!(SimAccount::from_seed(0).username, "sim_voter_00");
        assert_eq!(SimAccount::from_seed(1).username, "sim_voter_01");
        assert_eq!(SimAccount::from_seed(9).username, "sim_voter_09");
        assert_eq!(SimAccount::from_seed(10).username, "sim_voter_10");
        assert_eq!(SimAccount::from_seed(99).username, "sim_voter_99");
    }

    #[test]
    fn account_id_starts_none() {
        let account = SimAccount::from_seed(0);
        assert!(account.account_id.is_none());
    }

    #[test]
    fn signup_json_is_valid() {
        let account = SimAccount::from_seed(0);
        let json = account.build_signup_json().unwrap();

        let parsed: serde_json::Value =
            serde_json::from_str(&json).expect("signup JSON must be valid");

        assert_eq!(parsed["username"], "sim_voter_00");
        assert!(parsed["root_pubkey"].is_string());
        assert!(parsed["backup"]["encrypted_blob"].is_string());
        assert!(parsed["device"]["pubkey"].is_string());
        assert_eq!(parsed["device"]["name"], "Sim Device");
        assert!(parsed["device"]["certificate"].is_string());
    }

    #[test]
    fn signup_json_deterministic() {
        let a = SimAccount::from_seed(5);
        let b = SimAccount::from_seed(5);
        assert_eq!(
            a.build_signup_json().unwrap(),
            b.build_signup_json().unwrap()
        );
    }

    #[test]
    fn signup_json_certificate_verifies() {
        let account = SimAccount::from_seed(0);
        let json = account.build_signup_json().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        let root_pubkey_b64 = parsed["root_pubkey"].as_str().unwrap();
        let root_pubkey_bytes = tc_crypto::decode_base64url(root_pubkey_b64).unwrap();

        let device_pubkey_b64 = parsed["device"]["pubkey"].as_str().unwrap();
        let device_pubkey_bytes = tc_crypto::decode_base64url(device_pubkey_b64).unwrap();

        let cert_b64 = parsed["device"]["certificate"].as_str().unwrap();
        let cert_bytes = tc_crypto::decode_base64url(cert_b64).unwrap();

        // The certificate is the root key's signature over the device pubkey bytes.
        tc_crypto::verify_ed25519(
            root_pubkey_bytes.as_slice().try_into().unwrap(),
            &device_pubkey_bytes,
            cert_bytes.as_slice().try_into().unwrap(),
        )
        .expect("certificate must verify against root pubkey");
    }

    #[test]
    fn sign_request_produces_four_headers() {
        let account = SimAccount::from_seed(0);
        let headers = account.sign_request("GET", "/api/v1/rooms", b"");
        assert_eq!(headers.len(), 4);

        let names: Vec<&str> = headers.iter().map(|(name, _)| *name).collect();
        assert_eq!(
            names,
            vec!["X-Device-Kid", "X-Signature", "X-Timestamp", "X-Nonce"]
        );
    }

    #[test]
    fn sign_request_uses_device_kid() {
        let account = SimAccount::from_seed(0);
        let headers = account.sign_request("POST", "/api/v1/votes", b"{}");

        let kid_header = &headers[0];
        assert_eq!(kid_header.0, "X-Device-Kid");
        assert_eq!(kid_header.1, account.device_kid.to_string());
    }

    #[test]
    fn sign_request_signature_verifies() {
        let account = SimAccount::from_seed(0);
        let body = b"{\"poll_id\": \"abc\"}";
        let headers = account.sign_request("POST", "/api/v1/votes", body);

        let timestamp = &headers[2].1;
        let nonce = &headers[3].1;
        let sig_b64 = &headers[1].1;

        // Reconstruct the canonical message the same way the server would.
        let body_hash = Sha256::digest(body);
        let body_hash_hex = format!("{body_hash:x}");
        let canonical = format!("POST\n/api/v1/votes\n{timestamp}\n{nonce}\n{body_hash_hex}");

        let sig_bytes = tc_crypto::decode_base64url(sig_b64).unwrap();
        let device_pubkey_bytes = account.device_signing_key.verifying_key().to_bytes();

        tc_crypto::verify_ed25519(
            &device_pubkey_bytes,
            canonical.as_bytes(),
            sig_bytes.as_slice().try_into().unwrap(),
        )
        .expect("signature must verify with device public key");
    }

    #[test]
    fn sign_request_different_bodies_different_signatures() {
        let account = SimAccount::from_seed(0);
        let h1 = account.sign_request("POST", "/path", b"body1");
        let h2 = account.sign_request("POST", "/path", b"body2");

        // Signatures should differ (different body hashes, also different nonces/timestamps
        // but even with the same canonical construction the body hash would differ).
        assert_ne!(
            h1[1].1, h2[1].1,
            "different bodies must produce different signatures"
        );
    }

    #[test]
    fn signup_json_backup_envelope_parses() {
        let account = SimAccount::from_seed(0);
        let json = account.build_signup_json().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        let blob_b64 = parsed["backup"]["encrypted_blob"].as_str().unwrap();
        let blob_bytes = tc_crypto::decode_base64url(blob_b64).unwrap();

        let envelope = BackupEnvelope::parse(blob_bytes)
            .expect("backup blob must parse as a valid BackupEnvelope");
        assert_eq!(envelope.version(), 1);
        assert_eq!(envelope.salt(), &[0xAA; 16]);
    }

    #[test]
    fn verifier_is_deterministic() {
        let a = SimAccount::verifier();
        let b = SimAccount::verifier();
        assert_eq!(a.root_signing_key.to_bytes(), b.root_signing_key.to_bytes());
        assert_eq!(
            a.device_signing_key.to_bytes(),
            b.device_signing_key.to_bytes()
        );
        assert_eq!(a.device_kid, b.device_kid);
        assert_eq!(a.username, "sim_verifier");
    }

    #[test]
    fn verifier_keys_differ_from_voter_keys() {
        let verifier = SimAccount::verifier();
        let voter = SimAccount::from_seed(0);
        assert_ne!(
            verifier.root_signing_key.to_bytes(),
            voter.root_signing_key.to_bytes()
        );
        assert_ne!(
            verifier.device_signing_key.to_bytes(),
            voter.device_signing_key.to_bytes()
        );
    }

    #[test]
    fn root_pubkey_base64url_is_stable() {
        let a = SimAccount::verifier();
        let b = SimAccount::verifier();
        assert_eq!(a.root_pubkey_base64url(), b.root_pubkey_base64url());
        // 32 bytes → 43 base64url chars (no padding)
        assert_eq!(a.root_pubkey_base64url().len(), 43);
    }

    #[test]
    fn login_json_is_valid() {
        let account = SimAccount::from_seed(0);
        let json = account.build_login_json();
        let parsed: serde_json::Value =
            serde_json::from_str(&json).expect("login JSON must be valid");

        assert_eq!(parsed["username"], "sim_voter_00");
        assert!(parsed["timestamp"].is_number());
        assert!(parsed["device"]["pubkey"].is_string());
        assert_eq!(parsed["device"]["name"], "Sim Device");
        assert!(parsed["device"]["certificate"].is_string());
    }

    #[test]
    fn login_json_certificate_verifies() {
        let account = SimAccount::from_seed(0);
        let json = account.build_login_json();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        let root_pubkey_bytes = account.root_signing_key.verifying_key().to_bytes();

        let device_pubkey_b64 = parsed["device"]["pubkey"].as_str().unwrap();
        let device_pubkey_bytes = tc_crypto::decode_base64url(device_pubkey_b64).unwrap();

        let timestamp = parsed["timestamp"].as_i64().unwrap();
        let cert_b64 = parsed["device"]["certificate"].as_str().unwrap();
        let cert_bytes = tc_crypto::decode_base64url(cert_b64).unwrap();

        // Certificate signs device_pubkey || timestamp (LE i64 bytes)
        let mut signed_payload = Vec::with_capacity(40);
        signed_payload.extend_from_slice(&device_pubkey_bytes);
        signed_payload.extend_from_slice(&timestamp.to_le_bytes());

        tc_crypto::verify_ed25519(
            &root_pubkey_bytes,
            &signed_payload,
            cert_bytes.as_slice().try_into().unwrap(),
        )
        .expect("login certificate must verify against root pubkey");
    }

    #[test]
    fn keys_derived_via_sha256_not_trivial() {
        // Ensure we're not just using the index bytes directly.
        let account = SimAccount::from_seed(0);
        let root_bytes = account.root_signing_key.to_bytes();

        // The key should be the SHA-256 of "tc-sim-root-key-v1-0", not [0; 32].
        assert_ne!(root_bytes, [0u8; 32]);

        // Verify the derivation matches what we expect.
        let expected = Sha256::digest(b"tc-sim-root-key-v1-0");
        assert_eq!(&root_bytes[..], expected.as_slice());
    }

    #[test]
    fn demo_verifier_is_deterministic() {
        let a = SimAccount::demo_verifier();
        let b = SimAccount::demo_verifier();
        assert_eq!(a.root_signing_key.to_bytes(), b.root_signing_key.to_bytes());
        assert_eq!(
            a.device_signing_key.to_bytes(),
            b.device_signing_key.to_bytes()
        );
        assert_eq!(a.device_kid, b.device_kid);
        assert_eq!(a.username, "demo_verifier");
    }

    #[test]
    fn demo_verifier_keys_differ_from_sim_verifier() {
        let demo = SimAccount::demo_verifier();
        let sim = SimAccount::verifier();
        assert_ne!(
            demo.root_signing_key.to_bytes(),
            sim.root_signing_key.to_bytes()
        );
        assert_ne!(
            demo.device_signing_key.to_bytes(),
            sim.device_signing_key.to_bytes()
        );
    }
}
