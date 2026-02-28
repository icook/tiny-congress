//! Service layer for identity operations
//!
//! Provides the [`IdentityService`] trait that orchestrates validation,
//! account creation, backup storage, and device key registration.

use std::sync::Arc;

use async_trait::async_trait;
use serde::Deserialize;
use tc_crypto::{decode_base64url, verify_ed25519, BackupEnvelope, Kid};

use super::repo::{
    AccountRepoError, BackupRepoError, CreateSignupError, DeviceKeyRepoError, IdentityRepo,
    ValidatedSignup,
};

// Re-export repo's SignupResult — the service adds no extra fields today.
// If the service later needs its own fields (e.g., session tokens), fork it then.
pub use super::repo::SignupResult;

// ─── Domain request types ────────────────────────────────────────────────────

/// Backup data included in signup request
#[derive(Debug, Deserialize)]
pub struct SignupBackup {
    /// Base64url-encoded encrypted backup envelope
    pub encrypted_blob: String,
}

/// Device data included in signup request
#[derive(Debug, Deserialize)]
pub struct SignupDevice {
    /// Base64url-encoded Ed25519 public key
    pub pubkey: String,
    /// User-provided device name
    pub name: String,
    /// Base64url-encoded certificate (root key's signature over canonical cert message)
    pub certificate: String,
}

/// Signup request payload — atomic creation of account + backup + first device
#[derive(Debug, Deserialize)]
pub struct SignupRequest {
    pub username: String,
    pub root_pubkey: String, // base64url encoded
    pub backup: SignupBackup,
    pub device: SignupDevice,
}

// ─── Domain error type ──────────────────────────────────────────────────────

/// Error from signup, with variants that map cleanly to HTTP status codes.
#[derive(Debug, thiserror::Error)]
pub enum SignupError {
    #[error("{0}")]
    Validation(String),
    #[error("Username already taken")]
    DuplicateUsername,
    #[error("Public key already registered")]
    DuplicateKey,
    #[error("Maximum device limit reached")]
    MaxDevicesReached,
    #[error("internal error: {0}")]
    Internal(String),
}

// ─── Validation helpers ──────────────────────────────────────────────────────

const RESERVED_USERNAMES: &[&str] = &[
    "admin",
    "administrator",
    "root",
    "system",
    "mod",
    "moderator",
    "support",
    "help",
    "api",
    "graphql",
    "auth",
    "signup",
    "login",
    "null",
    "undefined",
    "anonymous",
];

/// Structured error type for username validation failures.
#[derive(Debug, PartialEq, Eq)]
pub enum UsernameError {
    Empty,
    TooShort,
    TooLong,
    InvalidCharacters,
    Reserved,
}

impl std::fmt::Display for UsernameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => write!(f, "Username cannot be empty"),
            Self::TooShort => write!(f, "Username must be at least 3 characters"),
            Self::TooLong => write!(f, "Username too long"),
            Self::InvalidCharacters => write!(
                f,
                "Username may only contain letters, numbers, hyphens, and underscores"
            ),
            Self::Reserved => write!(f, "This username is reserved"),
        }
    }
}

/// Validate a username, returning a structured error if invalid.
///
/// # Errors
///
/// Returns [`UsernameError`] if the username is empty, too short/long,
/// contains invalid characters, or is reserved.
pub fn validate_username(username: &str) -> Result<(), UsernameError> {
    if username.is_empty() {
        return Err(UsernameError::Empty);
    }
    if username.len() < 3 {
        return Err(UsernameError::TooShort);
    }
    if username.len() > 64 {
        return Err(UsernameError::TooLong);
    }
    if !username
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err(UsernameError::InvalidCharacters);
    }
    if RESERVED_USERNAMES.contains(&username.to_ascii_lowercase().as_str()) {
        return Err(UsernameError::Reserved);
    }
    Ok(())
}

// ─── Service trait and implementation ────────────────────────────────────────

/// Orchestrates the multi-step signup operation: validation + atomic persistence.
#[async_trait]
pub trait IdentityService: Send + Sync {
    async fn signup(&self, req: &SignupRequest) -> Result<SignupResult, SignupError>;
}

/// Production implementation — validates all fields then delegates to [`IdentityRepo`].
pub struct DefaultIdentityService {
    repo: Arc<dyn IdentityRepo>,
}

impl DefaultIdentityService {
    #[must_use]
    pub fn new(repo: Arc<dyn IdentityRepo>) -> Self {
        Self { repo }
    }
}

/// Map a [`CreateSignupError`] to a domain-level [`SignupError`].
fn map_signup_error(e: CreateSignupError) -> SignupError {
    match e {
        CreateSignupError::Account(AccountRepoError::DuplicateUsername) => {
            SignupError::DuplicateUsername
        }
        CreateSignupError::Account(AccountRepoError::DuplicateKey)
        | CreateSignupError::DeviceKey(DeviceKeyRepoError::DuplicateKid) => {
            SignupError::DuplicateKey
        }
        CreateSignupError::Account(AccountRepoError::NotFound) => {
            tracing::error!("Unexpected NotFound from account create during signup");
            SignupError::Internal("Internal server error".to_string())
        }
        CreateSignupError::DeviceKey(DeviceKeyRepoError::MaxDevicesReached) => {
            SignupError::MaxDevicesReached
        }
        CreateSignupError::Account(AccountRepoError::Database(e)) => {
            tracing::error!("Signup failed (account): {e}");
            SignupError::Internal("Internal server error".to_string())
        }
        CreateSignupError::Backup(BackupRepoError::DuplicateKid) => SignupError::DuplicateKey,
        CreateSignupError::Backup(BackupRepoError::DuplicateAccount) => {
            tracing::error!(
                "Unexpected DuplicateAccount on backup create during signup — account was just inserted"
            );
            SignupError::Internal("Internal server error".to_string())
        }
        CreateSignupError::Backup(BackupRepoError::Database(e)) => {
            tracing::error!("Signup failed (backup): {e}");
            SignupError::Internal("Internal server error".to_string())
        }
        CreateSignupError::Backup(BackupRepoError::NotFound) => {
            tracing::error!("Unexpected NotFound from backup create during signup");
            SignupError::Internal("Internal server error".to_string())
        }
        CreateSignupError::DeviceKey(
            DeviceKeyRepoError::NotFound | DeviceKeyRepoError::AlreadyRevoked,
        ) => {
            tracing::error!(
                "Unexpected NotFound/AlreadyRevoked from device key create during signup"
            );
            SignupError::Internal("Internal server error".to_string())
        }
        CreateSignupError::DeviceKey(DeviceKeyRepoError::Database(e)) => {
            tracing::error!("Signup failed (device key): {e}");
            SignupError::Internal("Internal server error".to_string())
        }
        CreateSignupError::Transaction(e) => {
            tracing::error!("Signup transaction failed: {e}");
            SignupError::Internal("Internal server error".to_string())
        }
    }
}

#[async_trait]
impl IdentityService for DefaultIdentityService {
    async fn signup(&self, req: &SignupRequest) -> Result<SignupResult, SignupError> {
        // Validate username
        let username = req.username.trim().to_string();
        validate_username(&username).map_err(|e| SignupError::Validation(e.to_string()))?;

        // Decode and validate root public key
        let root_pubkey_bytes = decode_base64url(&req.root_pubkey).map_err(|_| {
            SignupError::Validation("Invalid base64url encoding for root_pubkey".to_string())
        })?;
        let root_pubkey_arr: [u8; 32] = root_pubkey_bytes.as_slice().try_into().map_err(|_| {
            SignupError::Validation("root_pubkey must be 32 bytes (Ed25519)".to_string())
        })?;
        let root_kid = Kid::derive(&root_pubkey_arr);

        // Decode and validate encrypted backup
        let backup_bytes = decode_base64url(&req.backup.encrypted_blob).map_err(|_| {
            SignupError::Validation(
                "Invalid base64url encoding for backup.encrypted_blob".to_string(),
            )
        })?;
        let envelope = BackupEnvelope::parse(backup_bytes)
            .map_err(|e| SignupError::Validation(e.to_string()))?;

        // Decode and validate device public key
        let device_pubkey_bytes = decode_base64url(&req.device.pubkey).map_err(|_| {
            SignupError::Validation("Invalid base64url encoding for device.pubkey".to_string())
        })?;
        if device_pubkey_bytes.len() != 32 {
            return Err(SignupError::Validation(
                "device.pubkey must be 32 bytes (Ed25519)".to_string(),
            ));
        }
        let device_kid = Kid::derive(&device_pubkey_bytes);

        // Validate device name
        let device_name = req.device.name.trim().to_string();
        if device_name.is_empty() {
            return Err(SignupError::Validation(
                "Device name cannot be empty".to_string(),
            ));
        }
        if device_name.len() > 128 {
            return Err(SignupError::Validation("Device name too long".to_string()));
        }

        // Decode and verify certificate
        let certificate_bytes = decode_base64url(&req.device.certificate).map_err(|_| {
            SignupError::Validation("Invalid base64url encoding for device.certificate".to_string())
        })?;
        let cert_arr: [u8; 64] = certificate_bytes.as_slice().try_into().map_err(|_| {
            SignupError::Validation(
                "device.certificate must be 64 bytes (Ed25519 signature)".to_string(),
            )
        })?;

        // Verify the certificate: root key must have signed the device public key.
        // The signed message is the raw 32-byte device pubkey. This is sufficient because
        // device KIDs are globally unique (enforced by DB constraint), so a certificate
        // cannot be replayed for a different device. If a future "rotate device key"
        // feature reuses key material, the message format must be extended (e.g. with
        // account binding or a nonce).
        verify_ed25519(&root_pubkey_arr, &device_pubkey_bytes, &cert_arr)
            .map_err(|_| SignupError::Validation("Invalid device certificate".to_string()))?;

        // Build validated signup data and delegate to repo
        let validated = ValidatedSignup {
            username,
            root_pubkey: req.root_pubkey.clone(),
            root_kid,
            backup_bytes: envelope.as_bytes().to_vec(),
            backup_salt: envelope.salt().to_vec(),
            backup_version: envelope.version(),
            device_pubkey: req.device.pubkey.clone(),
            device_kid,
            device_name,
            certificate: certificate_bytes,
        };

        self.repo
            .create_signup(&validated)
            .await
            .map_err(map_signup_error)
    }
}

// ─── Mock for handler tests ──────────────────────────────────────────────────

#[cfg(any(test, feature = "test-utils"))]
#[allow(clippy::expect_used)]
pub mod mock {
    //! Mock identity service for HTTP handler unit tests.

    use super::{async_trait, IdentityService, SignupError, SignupRequest, SignupResult};
    use std::sync::Mutex;
    use tc_crypto::Kid;
    use uuid::Uuid;

    /// Mock service with a configurable signup result.
    ///
    /// For validation tests, use `DefaultIdentityService` with `MockIdentityRepo`.
    /// This mock is for handler tests that need to verify HTTP status code mapping.
    pub struct MockIdentityService {
        pub signup_result: Mutex<Option<Result<SignupResult, SignupError>>>,
    }

    impl MockIdentityService {
        #[must_use]
        pub const fn new() -> Self {
            Self {
                signup_result: Mutex::new(None),
            }
        }

        /// Create a mock that returns `Ok(SignupResult)` with default values.
        #[must_use]
        pub fn succeeding() -> Self {
            let mock = Self::new();
            mock.set_signup_result(Ok(SignupResult {
                account_id: Uuid::new_v4(),
                root_kid: Kid::derive(&[0u8; 32]),
                device_kid: Kid::derive(&[1u8; 32]),
            }));
            mock
        }

        /// Set the result that `signup()` will return.
        ///
        /// # Panics
        ///
        /// Panics if the internal mutex is poisoned.
        pub fn set_signup_result(&self, result: Result<SignupResult, SignupError>) {
            *self.signup_result.lock().expect("lock poisoned") = Some(result);
        }
    }

    impl Default for MockIdentityService {
        fn default() -> Self {
            Self::succeeding()
        }
    }

    #[async_trait]
    impl IdentityService for MockIdentityService {
        async fn signup(&self, _req: &SignupRequest) -> Result<SignupResult, SignupError> {
            self.signup_result
                .lock()
                .expect("lock poisoned")
                .take()
                .unwrap_or_else(|| {
                    Ok(SignupResult {
                        account_id: Uuid::new_v4(),
                        root_kid: Kid::derive(&[0u8; 32]),
                        device_kid: Kid::derive(&[1u8; 32]),
                    })
                })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::repo::mock::MockIdentityRepo;
    use ed25519_dalek::{Signer, SigningKey};
    use rand::rngs::OsRng;
    use tc_crypto::encode_base64url;

    fn test_envelope() -> BackupEnvelope {
        BackupEnvelope::build(
            [0xAA; 16], // salt
            65536,
            3,
            1,           // m_cost, t_cost, p_cost
            [0xBB; 12],  // nonce
            &[0xCC; 48], // ciphertext
        )
        .expect("test envelope")
    }

    fn valid_signup_request() -> SignupRequest {
        let root_signing_key = SigningKey::generate(&mut OsRng);
        let root_pubkey_bytes = root_signing_key.verifying_key().to_bytes();

        let device_signing_key = SigningKey::generate(&mut OsRng);
        let device_pubkey_bytes = device_signing_key.verifying_key().to_bytes();

        let certificate_sig = root_signing_key.sign(&device_pubkey_bytes);

        SignupRequest {
            username: "alice".to_string(),
            root_pubkey: encode_base64url(&root_pubkey_bytes),
            backup: SignupBackup {
                encrypted_blob: encode_base64url(test_envelope().as_bytes()),
            },
            device: SignupDevice {
                pubkey: encode_base64url(&device_pubkey_bytes),
                name: "Test Device".to_string(),
                certificate: encode_base64url(&certificate_sig.to_bytes()),
            },
        }
    }

    fn service_with_mock_repo() -> DefaultIdentityService {
        DefaultIdentityService::new(Arc::new(MockIdentityRepo::default()))
    }

    // ── Username validation (direct function tests) ────────────────────────

    #[test]
    fn test_validate_username_too_short() {
        assert_eq!(validate_username("ab"), Err(UsernameError::TooShort));
        assert_eq!(validate_username("a"), Err(UsernameError::TooShort));
    }

    #[test]
    fn test_validate_username_min_valid_length() {
        assert!(validate_username("abc").is_ok());
    }

    #[test]
    fn test_validate_username_too_long() {
        let long = "a".repeat(65);
        assert_eq!(validate_username(&long), Err(UsernameError::TooLong));
    }

    #[test]
    fn test_validate_username_max_valid_length() {
        let max = "a".repeat(64);
        assert!(validate_username(&max).is_ok());
    }

    #[test]
    fn test_validate_username_invalid_chars() {
        assert_eq!(
            validate_username("al!ce"),
            Err(UsernameError::InvalidCharacters)
        );
    }

    #[test]
    fn test_validate_username_unicode_rejected() {
        assert_eq!(
            validate_username("álice"),
            Err(UsernameError::InvalidCharacters)
        );
    }

    #[test]
    fn test_validate_username_spaces_rejected() {
        assert_eq!(
            validate_username("al ice"),
            Err(UsernameError::InvalidCharacters)
        );
    }

    #[test]
    fn test_validate_username_hyphens_underscores_valid() {
        assert!(validate_username("a-b_c").is_ok());
    }

    #[test]
    fn test_validate_username_reserved() {
        assert_eq!(validate_username("admin"), Err(UsernameError::Reserved));
    }

    #[test]
    fn test_validate_username_reserved_case_insensitive() {
        assert_eq!(validate_username("Admin"), Err(UsernameError::Reserved));
        assert_eq!(validate_username("ROOT"), Err(UsernameError::Reserved));
    }

    // ── Service-level validation tests ─────────────────────────────────────

    #[tokio::test]
    async fn test_signup_empty_username() {
        let svc = service_with_mock_repo();
        let mut req = valid_signup_request();
        req.username = String::new();
        let err = svc.signup(&req).await.unwrap_err();
        assert!(matches!(err, SignupError::Validation(_)));
    }

    #[tokio::test]
    async fn test_signup_invalid_root_pubkey() {
        let svc = service_with_mock_repo();
        let mut req = valid_signup_request();
        req.root_pubkey = "!!!not-base64!!!".to_string();
        let err = svc.signup(&req).await.unwrap_err();
        match &err {
            SignupError::Validation(msg) => assert!(msg.contains("root_pubkey")),
            other => panic!("expected Validation, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_signup_short_root_pubkey() {
        let svc = service_with_mock_repo();
        let mut req = valid_signup_request();
        req.root_pubkey = encode_base64url(&[1u8; 4]);
        let err = svc.signup(&req).await.unwrap_err();
        match &err {
            SignupError::Validation(msg) => assert!(msg.contains("32 bytes")),
            other => panic!("expected Validation, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_signup_invalid_backup_envelope() {
        let svc = service_with_mock_repo();
        let mut req = valid_signup_request();
        req.backup.encrypted_blob = encode_base64url(&[0u8; 10]);
        let err = svc.signup(&req).await.unwrap_err();
        match &err {
            SignupError::Validation(msg) => assert!(msg.contains("envelope")),
            other => panic!("expected Validation, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_signup_invalid_device_pubkey() {
        let svc = service_with_mock_repo();
        let mut req = valid_signup_request();
        req.device.pubkey = encode_base64url(&[2u8; 16]);
        let err = svc.signup(&req).await.unwrap_err();
        match &err {
            SignupError::Validation(msg) => assert!(msg.contains("device.pubkey")),
            other => panic!("expected Validation, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_signup_empty_device_name() {
        let svc = service_with_mock_repo();
        let mut req = valid_signup_request();
        req.device.name = String::new();
        let err = svc.signup(&req).await.unwrap_err();
        assert!(matches!(err, SignupError::Validation(_)));
    }

    #[tokio::test]
    async fn test_signup_invalid_certificate_length() {
        let svc = service_with_mock_repo();
        let mut req = valid_signup_request();
        req.device.certificate = encode_base64url(&[3u8; 32]);
        let err = svc.signup(&req).await.unwrap_err();
        match &err {
            SignupError::Validation(msg) => assert!(msg.contains("certificate")),
            other => panic!("expected Validation, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_signup_invalid_certificate_signature() {
        let svc = service_with_mock_repo();
        let mut req = valid_signup_request();
        req.device.certificate = encode_base64url(&[0xFFu8; 64]);
        let err = svc.signup(&req).await.unwrap_err();
        match &err {
            SignupError::Validation(msg) => assert!(msg.contains("Invalid device certificate")),
            other => panic!("expected Validation, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_signup_valid_request_succeeds() {
        let svc = service_with_mock_repo();
        let req = valid_signup_request();
        let result = svc.signup(&req).await;
        assert!(result.is_ok());
    }

    // ── Error mapping from repo → service ──────────────────────────────────

    #[tokio::test]
    async fn test_signup_duplicate_username_maps_correctly() {
        let repo = MockIdentityRepo::new();
        repo.set_signup_result(Err(CreateSignupError::Account(
            AccountRepoError::DuplicateUsername,
        )));
        let svc = DefaultIdentityService::new(Arc::new(repo));
        let err = svc.signup(&valid_signup_request()).await.unwrap_err();
        assert!(matches!(err, SignupError::DuplicateUsername));
    }

    #[tokio::test]
    async fn test_signup_duplicate_key_maps_correctly() {
        let repo = MockIdentityRepo::new();
        repo.set_signup_result(Err(CreateSignupError::Account(
            AccountRepoError::DuplicateKey,
        )));
        let svc = DefaultIdentityService::new(Arc::new(repo));
        let err = svc.signup(&valid_signup_request()).await.unwrap_err();
        assert!(matches!(err, SignupError::DuplicateKey));
    }

    #[tokio::test]
    async fn test_signup_device_kid_duplicate_maps_to_duplicate_key() {
        let repo = MockIdentityRepo::new();
        repo.set_signup_result(Err(CreateSignupError::DeviceKey(
            DeviceKeyRepoError::DuplicateKid,
        )));
        let svc = DefaultIdentityService::new(Arc::new(repo));
        let err = svc.signup(&valid_signup_request()).await.unwrap_err();
        assert!(matches!(err, SignupError::DuplicateKey));
    }

    #[tokio::test]
    async fn test_signup_max_devices_maps_correctly() {
        let repo = MockIdentityRepo::new();
        repo.set_signup_result(Err(CreateSignupError::DeviceKey(
            DeviceKeyRepoError::MaxDevicesReached,
        )));
        let svc = DefaultIdentityService::new(Arc::new(repo));
        let err = svc.signup(&valid_signup_request()).await.unwrap_err();
        assert!(matches!(err, SignupError::MaxDevicesReached));
    }

    #[tokio::test]
    async fn test_signup_database_error_maps_to_internal() {
        let repo = MockIdentityRepo::new();
        repo.set_signup_result(Err(CreateSignupError::Account(AccountRepoError::Database(
            sqlx::Error::Protocol("secret_password@db-host:5432".to_string()),
        ))));
        let svc = DefaultIdentityService::new(Arc::new(repo));
        let err = svc.signup(&valid_signup_request()).await.unwrap_err();
        match &err {
            SignupError::Internal(msg) => {
                assert!(!msg.contains("secret_password"));
                assert!(!msg.contains("db-host"));
            }
            other => panic!("expected Internal, got: {other:?}"),
        }
    }
}
