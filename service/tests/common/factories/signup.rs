//! Signup request factory for integration tests.

use ed25519_dalek::{Signer, SigningKey};
use rand::rngs::OsRng;
use tc_crypto::{encode_base64url, BackupEnvelope, Kid};

/// Keys generated during signup, needed for subsequent authenticated requests.
pub struct SignupKeys {
    pub root_signing_key: SigningKey,
    pub device_signing_key: SigningKey,
    pub device_kid: Kid,
}

/// Build a valid signup JSON body with real Ed25519 keys and certificate.
///
/// Generates fresh keypairs on each call so concurrent tests don't collide.
pub fn valid_signup_json(username: &str) -> String {
    let (json, _keys) = valid_signup_with_keys(username);
    json
}

/// Build a valid signup JSON body and return the signing keys for use in
/// subsequent authenticated requests.
pub fn valid_signup_with_keys(username: &str) -> (String, SignupKeys) {
    let root_signing_key = SigningKey::generate(&mut OsRng);
    let root_pubkey_bytes = root_signing_key.verifying_key().to_bytes();
    let root_pubkey = encode_base64url(&root_pubkey_bytes);

    let device_signing_key = SigningKey::generate(&mut OsRng);
    let device_pubkey_bytes = device_signing_key.verifying_key().to_bytes();
    let device_pubkey = encode_base64url(&device_pubkey_bytes);
    let device_kid = Kid::derive(&device_pubkey_bytes);

    let certificate_sig = root_signing_key.sign(&device_pubkey_bytes);
    let certificate = encode_base64url(&certificate_sig.to_bytes());

    let envelope = BackupEnvelope::build(
        [0xAA; 16], // salt
        65536,
        3,
        1,           // m_cost, t_cost, p_cost
        [0xBB; 12],  // nonce
        &[0xCC; 48], // ciphertext
    )
    .expect("test envelope");
    let backup_blob = encode_base64url(envelope.as_bytes());

    let json = format!(
        r#"{{"username": "{username}", "root_pubkey": "{root_pubkey}", "backup": {{"encrypted_blob": "{backup_blob}"}}, "device": {{"pubkey": "{device_pubkey}", "name": "Test Device", "certificate": "{certificate}"}}}}"#
    );

    let keys = SignupKeys {
        root_signing_key,
        device_signing_key,
        device_kid,
    };

    (json, keys)
}
