# M3: Login Flow + Real Backup Encryption — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add login/recovery flow, real Argon2id + ChaCha20-Poly1305 backup encryption, IndexedDB device persistence, and replay prevention.

**Architecture:** Two new unauthenticated backend endpoints (GET /auth/backup/:username, POST /auth/login) enable the client to fetch an encrypted backup, decrypt it with a password, and authorize a new device. The `AuthenticatedDevice` extractor gains nonce-based replay prevention via an in-memory store. The frontend DeviceProvider persists credentials to IndexedDB via `idb`.

**Tech Stack:** Rust/Axum (backend), React/Mantine/TanStack Query (frontend), `hash-wasm` (Argon2id), `@noble/ciphers` (ChaCha20-Poly1305), `idb` (IndexedDB)

**Base branch:** `feature/device-key-auth-m2` (M2 must merge first, then rebase onto master)

---

## Task 1: Account lookup by username (repo layer)

**Files:**
- Modify: `service/src/identity/repo/accounts.rs`
- Modify: `service/src/identity/repo/mod.rs`

**Step 1: Add `get_account_by_username` function**

In `service/src/identity/repo/accounts.rs`, add after the `get_account_by_id` function (after line 194):

```rust
/// Look up an account by username.
///
/// # Errors
///
/// Returns `AccountRepoError::NotFound` if no account matches.
pub async fn get_account_by_username<'e, E>(
    executor: E,
    username: &str,
) -> Result<AccountRecord, AccountRepoError>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let row = sqlx::query_as::<_, AccountRow>(
        r"
        SELECT id, username, root_pubkey, root_kid
        FROM accounts
        WHERE username = $1
        ",
    )
    .bind(username)
    .fetch_optional(executor)
    .await?;

    match row {
        Some(r) => {
            let root_kid: Kid = r
                .root_kid
                .parse()
                .map_err(|e| sqlx::Error::Decode(Box::new(e)))?;
            Ok(AccountRecord {
                id: r.id,
                username: r.username,
                root_pubkey: r.root_pubkey,
                root_kid,
            })
        }
        None => Err(AccountRepoError::NotFound),
    }
}
```

**Step 2: Re-export from mod.rs**

In `service/src/identity/repo/mod.rs`, add `get_account_by_username` to the `accounts` re-exports:

```rust
pub use accounts::{
    create_account_with_executor, get_account_by_id, get_account_by_username, AccountRecord,
    AccountRepo, AccountRepoError, CreatedAccount, PgAccountRepo,
};
```

**Step 3: Run tests**

Run: `cd service && cargo test --test identity_repo_tests -v`
Expected: all existing repo tests pass (no new tests needed — the function mirrors `get_account_by_id`)

**Step 4: Commit**

```bash
git add service/src/identity/repo/accounts.rs service/src/identity/repo/mod.rs
git commit -m "feat(identity): Add get_account_by_username repo function"
```

---

## Task 2: Backup retrieval endpoint

**Files:**
- Create: `service/src/identity/http/backup.rs`
- Modify: `service/src/identity/http/mod.rs` (add `mod backup;`, extend router)

**Step 1: Create `backup.rs` handler**

Create `service/src/identity/http/backup.rs`:

```rust
//! Backup retrieval endpoint for login/recovery flow.
//!
//! GET /auth/backup/:username — returns the encrypted backup envelope
//! so the client can decrypt it with the user's password and authorize
//! a new device.

use axum::{extract::Extension, extract::Path, http::StatusCode, response::IntoResponse, Json};
use serde::Serialize;
use sqlx::PgPool;
use tc_crypto::Kid;

use super::ErrorResponse;
use crate::identity::repo::{
    get_account_by_username, AccountRepoError, BackupRepoError, PgBackupRepo, BackupRepo,
};

#[derive(Debug, Serialize)]
pub struct BackupResponse {
    pub encrypted_backup: String,
    pub root_kid: Kid,
}

/// GET /auth/backup/:username — fetch encrypted backup for login
pub async fn get_backup(
    Extension(pool): Extension<PgPool>,
    Path(username): Path<String>,
) -> impl IntoResponse {
    let username = username.trim();
    if username.is_empty() {
        return super::bad_request("Username cannot be empty");
    }

    let account = match get_account_by_username(&pool, username).await {
        Ok(a) => a,
        Err(AccountRepoError::NotFound) => {
            return not_found("Account not found");
        }
        Err(e) => {
            tracing::error!("Failed to look up account: {e}");
            return internal_error();
        }
    };

    let backup_repo = PgBackupRepo::new(pool);
    match backup_repo.get_by_kid(&account.root_kid).await {
        Ok(record) => {
            let encrypted_backup =
                tc_crypto::encode_base64url(&record.encrypted_backup);
            (
                StatusCode::OK,
                Json(BackupResponse {
                    encrypted_backup,
                    root_kid: account.root_kid,
                }),
            )
                .into_response()
        }
        Err(BackupRepoError::NotFound) => not_found("Backup not found"),
        Err(e) => {
            tracing::error!("Failed to fetch backup: {e}");
            internal_error()
        }
    }
}

fn not_found(msg: &str) -> axum::response::Response {
    (
        StatusCode::NOT_FOUND,
        Json(ErrorResponse {
            error: msg.to_string(),
        }),
    )
        .into_response()
}

fn internal_error() -> axum::response::Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ErrorResponse {
            error: "Internal server error".to_string(),
        }),
    )
        .into_response()
}
```

**Step 2: Wire into router**

In `service/src/identity/http/mod.rs`:

Add module declaration (after line 4):
```rust
pub mod backup;
```

Add route to router (after the `/auth/signup` line, before the `/auth/devices` block):
```rust
.route("/auth/backup/{username}", get(backup::get_backup))
```

Add `get` to routing import if not already there (line 10 already has it).

**Step 3: Run lint and unit tests**

Run: `cd service && cargo clippy --all-features -- -D warnings && cargo test --lib -v`
Expected: compiles clean, unit tests pass

**Step 4: Commit**

```bash
git add service/src/identity/http/backup.rs service/src/identity/http/mod.rs
git commit -m "feat(identity): Add GET /auth/backup/:username endpoint"
```

---

## Task 3: Login endpoint

**Files:**
- Create: `service/src/identity/http/login.rs`
- Modify: `service/src/identity/http/mod.rs`

**Step 1: Create `login.rs` handler**

Create `service/src/identity/http/login.rs`:

```rust
//! Login endpoint — authorize a new device using root-key-signed certificate.
//!
//! POST /auth/login — unauthenticated. The client proves knowledge of the
//! root private key by presenting a certificate (root key's signature over
//! the new device's public key). The server verifies against the stored
//! root_pubkey and creates the device key.

use axum::{extract::Extension, http::StatusCode, response::IntoResponse, Json};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use super::ErrorResponse;
use crate::identity::repo::{
    create_device_key_with_executor, get_account_by_username, AccountRepoError,
    DeviceKeyRepoError,
};
use tc_crypto::{decode_base64url_native as decode_base64url, verify_ed25519, Kid};

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub device: LoginDevice,
}

#[derive(Debug, Deserialize)]
pub struct LoginDevice {
    pub pubkey: String,
    pub name: String,
    pub certificate: String,
}

#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub account_id: Uuid,
    pub root_kid: Kid,
    pub device_kid: Kid,
}

/// POST /auth/login — authorize new device via root key certificate
pub async fn login(
    Extension(pool): Extension<PgPool>,
    Json(req): Json<LoginRequest>,
) -> impl IntoResponse {
    let username = req.username.trim();
    if username.is_empty() {
        return bad_request("Username cannot be empty");
    }

    // Validate device pubkey
    let Ok(device_pubkey_bytes) = decode_base64url(&req.device.pubkey) else {
        return bad_request("Invalid base64url encoding for pubkey");
    };
    if device_pubkey_bytes.len() != 32 {
        return bad_request("pubkey must be 32 bytes (Ed25519)");
    }

    // Validate device name
    let device_name = req.device.name.trim();
    if device_name.is_empty() {
        return bad_request("Device name cannot be empty");
    }
    if device_name.chars().count() > 128 {
        return bad_request("Device name too long");
    }

    // Validate certificate
    let Ok(cert_bytes) = decode_base64url(&req.device.certificate) else {
        return bad_request("Invalid base64url encoding for certificate");
    };
    let Ok(cert_arr): Result<[u8; 64], _> = cert_bytes.as_slice().try_into() else {
        return bad_request("certificate must be 64 bytes (Ed25519 signature)");
    };

    // Look up account
    let account = match get_account_by_username(&pool, username).await {
        Ok(a) => a,
        Err(AccountRepoError::NotFound) => {
            return not_found("Account not found");
        }
        Err(e) => {
            tracing::error!("Failed to look up account: {e}");
            return internal_error();
        }
    };

    // Verify certificate against root pubkey
    let Ok(root_pubkey_bytes) = decode_base64url(&account.root_pubkey) else {
        tracing::error!("Corrupted root pubkey for account {}", account.id);
        return internal_error();
    };
    let Ok(root_pubkey_arr): Result<[u8; 32], _> = root_pubkey_bytes.as_slice().try_into() else {
        tracing::error!("Corrupted root pubkey length for account {}", account.id);
        return internal_error();
    };

    if verify_ed25519(&root_pubkey_arr, &device_pubkey_bytes, &cert_arr).is_err() {
        return bad_request("Invalid device certificate");
    }

    let device_kid = Kid::derive(&device_pubkey_bytes);

    // Create device key in a transaction
    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            tracing::error!("Failed to begin transaction: {e}");
            return internal_error();
        }
    };

    let created = match create_device_key_with_executor(
        &mut tx,
        account.id,
        &device_kid,
        &req.device.pubkey,
        device_name,
        &cert_bytes,
    )
    .await
    {
        Ok(c) => c,
        Err(DeviceKeyRepoError::DuplicateKid) => {
            return (
                StatusCode::CONFLICT,
                Json(ErrorResponse {
                    error: "Device key already registered".to_string(),
                }),
            )
                .into_response();
        }
        Err(DeviceKeyRepoError::MaxDevicesReached) => {
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(ErrorResponse {
                    error: "Maximum device limit reached".to_string(),
                }),
            )
                .into_response();
        }
        Err(e) => {
            tracing::error!("Failed to create device key: {e}");
            return internal_error();
        }
    };

    if let Err(e) = tx.commit().await {
        tracing::error!("Failed to commit login transaction: {e}");
        return internal_error();
    }

    (
        StatusCode::CREATED,
        Json(LoginResponse {
            account_id: account.id,
            root_kid: account.root_kid,
            device_kid: created.device_kid,
        }),
    )
        .into_response()
}

fn bad_request(msg: &str) -> axum::response::Response {
    super::bad_request(msg)
}

fn not_found(msg: &str) -> axum::response::Response {
    (
        StatusCode::NOT_FOUND,
        Json(ErrorResponse {
            error: msg.to_string(),
        }),
    )
        .into_response()
}

fn internal_error() -> axum::response::Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ErrorResponse {
            error: "Internal server error".to_string(),
        }),
    )
        .into_response()
}
```

**Step 2: Wire into router**

In `service/src/identity/http/mod.rs`:

Add module declaration:
```rust
pub mod login;
```

Add route to router:
```rust
.route("/auth/login", post(login::login))
```

**Step 3: Run lint**

Run: `cd service && cargo clippy --all-features -- -D warnings`
Expected: clean

**Step 4: Commit**

```bash
git add service/src/identity/http/login.rs service/src/identity/http/mod.rs
git commit -m "feat(identity): Add POST /auth/login endpoint"
```

---

## Task 4: Backend integration tests for backup + login

**Files:**
- Create: `service/tests/login_tests.rs`

**Step 1: Write integration tests**

Create `service/tests/login_tests.rs`:

```rust
//! Login and backup retrieval integration tests.

mod common;

use axum::{
    body::{to_bytes, Body},
    http::{header::CONTENT_TYPE, Method, Request, StatusCode},
};
use common::app_builder::TestAppBuilder;
use common::factories::{valid_signup_with_keys, SignupKeys};
use common::test_db::isolated_db;
use ed25519_dalek::{Signer, SigningKey};
use rand::rngs::OsRng;
use tc_crypto::{encode_base64url, Kid};
use tc_test_macros::shared_runtime_test;
use tower::ServiceExt;

/// Sign up a user in an isolated DB, return (app, keys).
async fn signup_user(
    username: &str,
) -> (axum::Router, SignupKeys, common::test_db::IsolatedDb) {
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

// =========================================================================
// GET /auth/backup/:username
// =========================================================================

#[shared_runtime_test]
async fn test_get_backup_success() {
    let (app, _keys, _db) = signup_user("backupget").await;

    let req = Request::builder()
        .method(Method::GET)
        .uri("/auth/backup/backupget")
        .body(Body::empty())
        .expect("request");

    let response = app.oneshot(req).await.expect("response");
    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let json: serde_json::Value = serde_json::from_slice(&body).expect("json");
    assert!(json["encrypted_backup"].is_string());
    assert!(json["root_kid"].is_string());
}

#[shared_runtime_test]
async fn test_get_backup_not_found() {
    let db = isolated_db().await;
    let app = TestAppBuilder::new()
        .with_identity_pool(db.pool().clone())
        .build();

    let req = Request::builder()
        .method(Method::GET)
        .uri("/auth/backup/nonexistent")
        .body(Body::empty())
        .expect("request");

    let response = app.oneshot(req).await.expect("response");
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

// =========================================================================
// POST /auth/login
// =========================================================================

#[shared_runtime_test]
async fn test_login_success() {
    let (app, keys, _db) = signup_user("loginuser").await;

    // Generate a new device key and sign its certificate with the root key
    let new_device_key = SigningKey::generate(&mut OsRng);
    let new_device_pubkey = new_device_key.verifying_key().to_bytes();
    let cert = keys.root_signing_key.sign(&new_device_pubkey);

    let body = serde_json::json!({
        "username": "loginuser",
        "device": {
            "pubkey": encode_base64url(&new_device_pubkey),
            "name": "Login Device",
            "certificate": encode_base64url(&cert.to_bytes()),
        }
    })
    .to_string();

    let req = Request::builder()
        .method(Method::POST)
        .uri("/auth/login")
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(body))
        .expect("request");

    let response = app.oneshot(req).await.expect("response");
    assert_eq!(response.status(), StatusCode::CREATED);

    let resp_body = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let json: serde_json::Value = serde_json::from_slice(&resp_body).expect("json");
    assert!(json["account_id"].is_string());
    assert!(json["root_kid"].is_string());
    assert!(json["device_kid"].is_string());
}

#[shared_runtime_test]
async fn test_login_unknown_username() {
    let db = isolated_db().await;
    let app = TestAppBuilder::new()
        .with_identity_pool(db.pool().clone())
        .build();

    let device_key = SigningKey::generate(&mut OsRng);
    let device_pubkey = device_key.verifying_key().to_bytes();
    // Certificate signed by a random key (not the account's root key)
    let fake_root = SigningKey::generate(&mut OsRng);
    let cert = fake_root.sign(&device_pubkey);

    let body = serde_json::json!({
        "username": "nobody",
        "device": {
            "pubkey": encode_base64url(&device_pubkey),
            "name": "Test",
            "certificate": encode_base64url(&cert.to_bytes()),
        }
    })
    .to_string();

    let req = Request::builder()
        .method(Method::POST)
        .uri("/auth/login")
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(body))
        .expect("request");

    let response = app.oneshot(req).await.expect("response");
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[shared_runtime_test]
async fn test_login_invalid_certificate() {
    let (app, _keys, _db) = signup_user("logincertfail").await;

    // Generate a device key but sign certificate with a DIFFERENT root key
    let new_device_key = SigningKey::generate(&mut OsRng);
    let new_device_pubkey = new_device_key.verifying_key().to_bytes();
    let wrong_root = SigningKey::generate(&mut OsRng);
    let bad_cert = wrong_root.sign(&new_device_pubkey);

    let body = serde_json::json!({
        "username": "logincertfail",
        "device": {
            "pubkey": encode_base64url(&new_device_pubkey),
            "name": "Bad Cert Device",
            "certificate": encode_base64url(&bad_cert.to_bytes()),
        }
    })
    .to_string();

    let req = Request::builder()
        .method(Method::POST)
        .uri("/auth/login")
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(body))
        .expect("request");

    let response = app.oneshot(req).await.expect("response");
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}
```

**Step 2: Run tests**

Run: `cd service && cargo test --test login_tests -v`
Expected: all 4 tests pass

**Step 3: Commit**

```bash
git add service/tests/login_tests.rs
git commit -m "test(identity): Add integration tests for backup retrieval and login"
```

---

## Task 5: Nonce-based replay prevention

**Files:**
- Create: `service/src/identity/http/nonce.rs`
- Modify: `service/src/identity/http/auth.rs`
- Modify: `service/src/identity/http/mod.rs`
- Modify: `service/src/main.rs`

**Step 1: Create NonceStore**

Create `service/src/identity/http/nonce.rs`:

```rust
//! In-memory nonce store for replay prevention.
//!
//! Each authenticated request must include a unique `X-Nonce` header value.
//! The server rejects any nonce it has seen within the timestamp validity
//! window (2 × MAX_TIMESTAMP_SKEW = 600 seconds).

use std::collections::HashMap;
use std::sync::RwLock;
use std::time::Instant;

/// Duration after which nonces are eligible for cleanup.
/// Set to 2× the max timestamp skew so nonces outlive the request window.
const NONCE_TTL_SECS: u64 = 600;

/// Maximum nonce length to prevent memory abuse.
const MAX_NONCE_LENGTH: usize = 64;

/// In-memory store tracking recently-seen request nonces.
///
/// Thread-safe via `RwLock`. Suitable for single-process deployment.
/// For multi-process, replace with Redis or similar.
#[derive(Debug)]
pub struct NonceStore {
    seen: RwLock<HashMap<String, Instant>>,
}

impl NonceStore {
    /// Create an empty nonce store.
    #[must_use]
    pub fn new() -> Self {
        Self {
            seen: RwLock::new(HashMap::new()),
        }
    }

    /// Check if a nonce has been seen. If not, record it and return `true`.
    /// Returns `false` if the nonce was already used (replay detected).
    ///
    /// Also performs lazy cleanup when the map exceeds 10 000 entries.
    ///
    /// # Panics
    ///
    /// Panics if the internal RwLock is poisoned.
    pub fn check_and_insert(&self, nonce: &str) -> bool {
        if nonce.is_empty() || nonce.len() > MAX_NONCE_LENGTH {
            return false;
        }

        let mut map = self.seen.write().expect("nonce store lock poisoned");

        // Lazy cleanup when map gets large
        if map.len() > 10_000 {
            let cutoff = Instant::now() - std::time::Duration::from_secs(NONCE_TTL_SECS);
            map.retain(|_, &mut ts| ts > cutoff);
        }

        if map.contains_key(nonce) {
            return false;
        }

        map.insert(nonce.to_string(), Instant::now());
        true
    }
}

impl Default for NonceStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_fresh_nonce() {
        let store = NonceStore::new();
        assert!(store.check_and_insert("nonce-1"));
    }

    #[test]
    fn rejects_duplicate_nonce() {
        let store = NonceStore::new();
        assert!(store.check_and_insert("nonce-dup"));
        assert!(!store.check_and_insert("nonce-dup"));
    }

    #[test]
    fn rejects_empty_nonce() {
        let store = NonceStore::new();
        assert!(!store.check_and_insert(""));
    }

    #[test]
    fn rejects_oversized_nonce() {
        let store = NonceStore::new();
        let long = "x".repeat(MAX_NONCE_LENGTH + 1);
        assert!(!store.check_and_insert(&long));
    }

    #[test]
    fn accepts_max_length_nonce() {
        let store = NonceStore::new();
        let exact = "x".repeat(MAX_NONCE_LENGTH);
        assert!(store.check_and_insert(&exact));
    }
}
```

**Step 2: Update AuthenticatedDevice extractor**

In `service/src/identity/http/auth.rs`:

Update the module doc comment (lines 1-14) — new canonical format:
```rust
//! Canonical message format:
//! ```text
//! {METHOD}\n{PATH_AND_QUERY}\n{TIMESTAMP}\n{NONCE}\n{BODY_SHA256_HEX}
//! ```
//!
//! Required headers:
//! - `X-Device-Kid`: 22-char base64url key identifier
//! - `X-Signature`: base64url Ed25519 signature of the canonical message
//! - `X-Timestamp`: Unix seconds
//! - `X-Nonce`: Unique request identifier (UUID recommended, max 64 chars)
```

In `from_request`, after extracting `X-Timestamp` (around line 108), add nonce extraction:

```rust
let nonce = req
    .headers()
    .get("X-Nonce")
    .and_then(|v| v.to_str().ok())
    .ok_or_else(|| auth_error("Missing X-Nonce header"))?
    .to_string();
```

After extracting the pool (around line 86), also extract the NonceStore:

```rust
use super::nonce::NonceStore;

let nonce_store = req
    .extensions()
    .get::<std::sync::Arc<NonceStore>>()
    .ok_or_else(|| auth_error("Server misconfiguration"))?
    .clone();
```

After timestamp validation, add nonce check:

```rust
// Check nonce for replay prevention
if !nonce_store.check_and_insert(&nonce) {
    return Err(auth_error("Duplicate nonce (possible replay)"));
}
```

Update canonical message format (line 153):
```rust
let canonical = format!("{method}\n{path}\n{timestamp}\n{nonce}\n{body_hash_hex}");
```

Update the `test_canonical_message_format` unit test to include nonce:
```rust
#[test]
fn test_canonical_message_format() {
    let method = "GET";
    let path = "/auth/devices";
    let timestamp = 1700000000_i64;
    let nonce = "test-nonce-uuid";
    let body_hash_hex = format!("{:x}", Sha256::digest(b""));

    let canonical = format!("{method}\n{path}\n{timestamp}\n{nonce}\n{body_hash_hex}");

    assert!(canonical.starts_with("GET\n/auth/devices\n1700000000\ntest-nonce-uuid\n"));
    assert!(
        canonical.ends_with("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855")
    );
}
```

**Step 3: Add NonceStore as Extension**

In `service/src/identity/http/mod.rs`, add module declaration:
```rust
pub mod nonce;
```

In `service/src/main.rs`, after the pool Extension layer, add:
```rust
use std::sync::Arc;
use tinycongress_api::identity::http::nonce::NonceStore;

// ... in the app builder ...
.layer(Extension(Arc::new(NonceStore::new())))
```

**Step 4: Update TestAppBuilder**

In `service/tests/common/app_builder.rs`, add the NonceStore extension in the `build()` method, alongside the pool extension:

```rust
use std::sync::Arc;
use tinycongress_api::identity::http::nonce::NonceStore;

// Inside build(), after .layer(Extension(pool.clone())):
.layer(Extension(Arc::new(NonceStore::new())))
```

**Step 5: Update `sign_request` in device_handler_tests.rs and login_tests.rs**

In `service/tests/device_handler_tests.rs`, update `sign_request` to include nonce:

```rust
fn sign_request(
    method: &str,
    path: &str,
    body: &[u8],
    signing_key: &SigningKey,
    kid: &Kid,
) -> Vec<(&'static str, String)> {
    let timestamp = chrono::Utc::now().timestamp();
    let nonce = uuid::Uuid::new_v4().to_string();
    let body_hash = Sha256::digest(body);
    let body_hash_hex = format!("{body_hash:x}");
    let canonical = format!("{method}\n{path}\n{timestamp}\n{nonce}\n{body_hash_hex}");
    let signature = signing_key.sign(canonical.as_bytes());

    vec![
        ("X-Device-Kid", kid.to_string()),
        ("X-Signature", encode_base64url(&signature.to_bytes())),
        ("X-Timestamp", timestamp.to_string()),
        ("X-Nonce", nonce),
    ]
}
```

Add `uuid` to dev-dependencies in `service/Cargo.toml` (it may already be there as a regular dep — check and add to `[dev-dependencies]` if needed):
```toml
uuid = { version = "1", features = ["v4"] }
```

Also update the `test_list_devices_expired_timestamp` test to include a nonce in its manual header construction.

**Step 6: Run all tests**

Run: `cd service && cargo test -v`
Expected: all tests pass (unit + integration)

**Step 7: Commit**

```bash
git add service/src/identity/http/nonce.rs service/src/identity/http/auth.rs \
    service/src/identity/http/mod.rs service/src/main.rs \
    service/tests/common/app_builder.rs service/tests/device_handler_tests.rs \
    service/Cargo.toml service/Cargo.lock
git commit -m "feat(identity): Add nonce-based replay prevention (#318)"
```

---

## Task 6: Replay prevention integration test

**Files:**
- Modify: `service/tests/device_handler_tests.rs`

**Step 1: Add replay test**

Add to `service/tests/device_handler_tests.rs`:

```rust
#[shared_runtime_test]
async fn test_nonce_replay_rejected() {
    let (app, keys, _db) = signup_user("noncereplay").await;

    // Build a request with a specific nonce
    let nonce = "fixed-nonce-for-replay-test";
    let timestamp = chrono::Utc::now().timestamp();
    let body_hash = Sha256::digest(b"");
    let body_hash_hex = format!("{body_hash:x}");
    let canonical = format!("GET\n/auth/devices\n{timestamp}\n{nonce}\n{body_hash_hex}");
    let signature = keys.device_signing_key.sign(canonical.as_bytes());

    let build_req = || {
        Request::builder()
            .method(Method::GET)
            .uri("/auth/devices")
            .header("X-Device-Kid", keys.device_kid.to_string())
            .header("X-Signature", encode_base64url(&signature.to_bytes()))
            .header("X-Timestamp", timestamp.to_string())
            .header("X-Nonce", nonce)
            .body(Body::empty())
            .expect("request")
    };

    // First request succeeds
    let response = app.clone().oneshot(build_req()).await.expect("response");
    assert_eq!(response.status(), StatusCode::OK);

    // Same nonce replayed — should be rejected
    let response = app.oneshot(build_req()).await.expect("response");
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
```

**Step 2: Run test**

Run: `cd service && cargo test --test device_handler_tests test_nonce_replay_rejected -v`
Expected: PASS

**Step 3: Commit**

```bash
git add service/tests/device_handler_tests.rs
git commit -m "test(identity): Add nonce replay rejection integration test"
```

---

## Task 7: Install frontend crypto dependencies

**Files:**
- Modify: `web/package.json`
- Modify: `web/yarn.lock`

**Step 1: Install packages**

Run:
```bash
cd web && yarn add hash-wasm @noble/ciphers idb
```

**Step 2: Verify install**

Run: `cd web && yarn vitest --run --reporter=verbose`
Expected: all existing tests pass

**Step 3: Commit**

```bash
git add web/package.json web/yarn.lock
git commit -m "deps(web): Add hash-wasm, @noble/ciphers, idb for M3 crypto and persistence"
```

---

## Task 8: Real backup encryption

**Files:**
- Modify: `web/src/features/identity/keys/crypto.ts`
- Modify: `web/src/features/identity/keys/crypto.test.ts`

**Step 1: Write failing tests**

In `web/src/features/identity/keys/crypto.test.ts` (or create a new test file `web/src/features/identity/keys/__tests__/backup.test.ts`), add:

```typescript
import { describe, expect, test } from 'vitest';
import { buildBackupEnvelope, decryptBackupEnvelope } from '../crypto';

describe('backup envelope encryption', () => {
  test('encrypt and decrypt roundtrip recovers the root key', async () => {
    const rootPrivateKey = globalThis.crypto.getRandomValues(new Uint8Array(32));
    const password = 'test-password-123';

    const envelope = await buildBackupEnvelope(rootPrivateKey, password);
    const recovered = await decryptBackupEnvelope(envelope, password);

    expect(recovered).toEqual(rootPrivateKey);
  });

  test('decrypt with wrong password throws', async () => {
    const rootPrivateKey = globalThis.crypto.getRandomValues(new Uint8Array(32));
    const envelope = await buildBackupEnvelope(rootPrivateKey, 'correct-password');

    await expect(decryptBackupEnvelope(envelope, 'wrong-password')).rejects.toThrow();
  });

  test('envelope has correct structure', async () => {
    const rootPrivateKey = globalThis.crypto.getRandomValues(new Uint8Array(32));
    const envelope = await buildBackupEnvelope(rootPrivateKey, 'test');

    // Minimum envelope size: 42 header + 48 ciphertext = 90
    expect(envelope.length).toBe(90);
    // Version byte
    expect(envelope[0]).toBe(0x01);
    // KDF byte (Argon2id)
    expect(envelope[1]).toBe(0x01);
    // m_cost = 65536
    const view = new DataView(envelope.buffer, envelope.byteOffset, envelope.byteLength);
    expect(view.getUint32(2, true)).toBe(65536);
    // t_cost = 3
    expect(view.getUint32(6, true)).toBe(3);
    // p_cost = 1
    expect(view.getUint32(10, true)).toBe(1);
  });
});
```

**Step 2: Run tests — verify they fail**

Run: `cd web && yarn vitest --run --reporter=verbose -- backup.test`
Expected: FAIL — `buildBackupEnvelope` has wrong signature (no params yet)

**Step 3: Implement real encryption**

Replace the `buildBackupEnvelope` function in `web/src/features/identity/keys/crypto.ts`:

```typescript
import { argon2id } from 'hash-wasm';
import { chacha20poly1305 } from '@noble/ciphers/chacha';

// ... existing imports ...

/**
 * KDF parameters matching BackupEnvelope constraints.
 * These are the minimums enforced by the Rust BackupEnvelope::parse.
 */
const KDF_M_COST = 65536; // 64 MiB
const KDF_T_COST = 3;
const KDF_P_COST = 1;
const KDF_HASH_LENGTH = 32;

/**
 * Build an encrypted backup envelope containing the root private key.
 *
 * Uses Argon2id for key derivation and ChaCha20-Poly1305 for encryption.
 * The envelope format matches the Rust BackupEnvelope binary layout.
 *
 * @param rootPrivateKey - 32-byte Ed25519 private key to encrypt
 * @param password - User's backup password
 * @returns Binary envelope (90 bytes: 42 header + 48 ciphertext)
 */
export async function buildBackupEnvelope(
  rootPrivateKey: Uint8Array,
  password: string
): Promise<Uint8Array> {
  const salt = globalThis.crypto.getRandomValues(new Uint8Array(16));
  const nonce = globalThis.crypto.getRandomValues(new Uint8Array(12));

  // Derive encryption key via Argon2id
  const keyBytes = await argon2id({
    password,
    salt,
    parallelism: KDF_P_COST,
    iterations: KDF_T_COST,
    memorySize: KDF_M_COST,
    hashLength: KDF_HASH_LENGTH,
    outputType: 'binary',
  });

  // Encrypt root private key with ChaCha20-Poly1305
  const cipher = chacha20poly1305(keyBytes, nonce);
  const ciphertext = cipher.encrypt(rootPrivateKey);

  // Assemble envelope: [version:1][kdf:1][m:4LE][t:4LE][p:4LE][salt:16][nonce:12][ciphertext:48]
  const envelope = new Uint8Array(42 + ciphertext.length);
  const view = new DataView(envelope.buffer);
  envelope[0] = 0x01; // version
  envelope[1] = 0x01; // kdf_id = Argon2id
  view.setUint32(2, KDF_M_COST, true);
  view.setUint32(6, KDF_T_COST, true);
  view.setUint32(10, KDF_P_COST, true);
  envelope.set(salt, 14);
  envelope.set(nonce, 30);
  envelope.set(ciphertext, 42);

  return envelope;
}

/**
 * Decrypt a backup envelope to recover the root private key.
 *
 * @param envelope - Binary envelope bytes (from server)
 * @param password - User's backup password
 * @returns 32-byte Ed25519 private key
 * @throws Error if password is wrong or envelope is corrupt
 */
export async function decryptBackupEnvelope(
  envelope: Uint8Array,
  password: string
): Promise<Uint8Array> {
  if (envelope.length < 90) {
    throw new Error('Backup envelope too small');
  }
  if (envelope[0] !== 0x01) {
    throw new Error('Unsupported envelope version');
  }
  if (envelope[1] !== 0x01) {
    throw new Error('Unsupported KDF');
  }

  const view = new DataView(envelope.buffer, envelope.byteOffset, envelope.byteLength);
  const mCost = view.getUint32(2, true);
  const tCost = view.getUint32(6, true);
  const pCost = view.getUint32(10, true);
  const salt = envelope.slice(14, 30);
  const nonce = envelope.slice(30, 42);
  const ciphertext = envelope.slice(42);

  // Derive decryption key via Argon2id
  const keyBytes = await argon2id({
    password,
    salt,
    parallelism: pCost,
    iterations: tCost,
    memorySize: mCost,
    hashLength: KDF_HASH_LENGTH,
    outputType: 'binary',
  });

  // Decrypt with ChaCha20-Poly1305
  const cipher = chacha20poly1305(keyBytes, nonce);
  const plaintext = cipher.decrypt(ciphertext);

  if (plaintext.length !== 32) {
    throw new Error('Decrypted key has unexpected length');
  }

  return plaintext;
}
```

**Step 4: Run tests — verify they pass**

Run: `cd web && yarn vitest --run --reporter=verbose -- backup.test`
Expected: all 3 tests PASS

Note: Argon2id with m=65536 allocates 64 MiB. If this causes test timeouts, add `{ timeout: 30000 }` to the describe block. If Vitest's default WASM support has issues with hash-wasm, check if a `vitest.config.ts` adjustment is needed.

**Step 5: Update barrel exports**

In `web/src/features/identity/keys/index.ts`, ensure `buildBackupEnvelope` and `decryptBackupEnvelope` are already exported (they come through `export * from './crypto'`).

**Step 6: Run lint**

Run: `just lint-frontend`
Expected: clean

**Step 7: Commit**

```bash
git add web/src/features/identity/keys/crypto.ts \
    web/src/features/identity/keys/__tests__/backup.test.ts
git commit -m "feat(identity): Real Argon2id + ChaCha20-Poly1305 backup encryption (#319)"
```

---

## Task 9: Update signup flow for real encryption

**Files:**
- Modify: `web/src/pages/Signup.page.tsx`
- Modify: `web/src/features/identity/components/SignupForm.tsx`
- Modify: `web/src/pages/Signup.page.test.tsx`

**Step 1: Add password field to SignupForm**

In `web/src/features/identity/components/SignupForm.tsx`, add password to props:

```typescript
export interface SignupFormProps {
  username: string;
  password: string;
  onUsernameChange: (value: string) => void;
  onPasswordChange: (value: string) => void;
  onSubmit: (e: React.FormEvent) => void;
  isLoading: boolean;
  loadingText?: string;
  error?: string | null;
  successData?: { account_id: string; root_kid: string; device_kid: string } | null;
}
```

Add a `PasswordInput` field after the username field:

```typescript
import { ..., PasswordInput } from '@mantine/core';

// In the form, after the username TextInput:
<PasswordInput
  label="Backup Password"
  description="Used to encrypt your root key backup. You'll need this to log in on new devices."
  required
  value={password}
  onChange={(e) => {
    onPasswordChange(e.currentTarget.value);
  }}
  disabled={isLoading}
/>
```

**Step 2: Thread password through Signup.page.tsx**

In `web/src/pages/Signup.page.tsx`:

Add password state:
```typescript
const [password, setPassword] = useState('');
```

Update `handleSubmit` — pass password to `buildBackupEnvelope`:
```typescript
const envelope = await buildBackupEnvelope(rootKeyPair.privateKey, password);
```

Since `buildBackupEnvelope` is now async, the existing `await signup.mutateAsync(...)` pattern works.

Update the returned `<SignupForm>` to include password props:
```typescript
<SignupForm
  username={username}
  password={password}
  onUsernameChange={setUsername}
  onPasswordChange={setPassword}
  // ... rest unchanged
/>
```

Add password validation in handleSubmit:
```typescript
if (!password) {
  return;
}
```

**Step 3: Update tests**

In `web/src/pages/Signup.page.test.tsx`, update the mock and test setup to include the password field. Mock `buildBackupEnvelope` to be async:

```typescript
vi.mock('@/features/identity', () => ({
  generateKeyPair: vi.fn(),
  signMessage: vi.fn(),
  buildBackupEnvelope: vi.fn().mockResolvedValue(new Uint8Array(90)),
  useSignup: vi.fn(),
}));
```

In each test that submits the form, fill in the password field:
```typescript
await user.type(screen.getByLabelText(/backup password/i), 'test-password');
```

**Step 4: Run tests**

Run: `cd web && yarn vitest --run --reporter=verbose -- Signup`
Expected: all tests pass

**Step 5: Run lint**

Run: `just lint-frontend`
Expected: clean

**Step 6: Commit**

```bash
git add web/src/pages/Signup.page.tsx \
    web/src/features/identity/components/SignupForm.tsx \
    web/src/pages/Signup.page.test.tsx
git commit -m "feat(identity): Thread password through signup for real backup encryption"
```

---

## Task 10: IndexedDB device persistence

**Files:**
- Modify: `web/src/providers/DeviceProvider.tsx`

**Step 1: Implement IndexedDB persistence**

Replace the DeviceProvider implementation:

```typescript
/**
 * Device context provider — persists device credentials in IndexedDB.
 *
 * On mount, reads credentials from IndexedDB. On setDevice/clearDevice,
 * writes/deletes from IndexedDB and updates React state.
 */

import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from 'react';
import { openDB, type IDBPDatabase } from 'idb';

const DB_NAME = 'tc-device-store';
const DB_VERSION = 1;
const STORE_NAME = 'device';
const CURRENT_KEY = 'current';

interface StoredDevice {
  kid: string;
  privateKey: Uint8Array;
}

interface DeviceContextValue {
  /** Current device KID, or null if not authenticated */
  deviceKid: string | null;
  /** Current device signing key, or null if not authenticated */
  privateKey: Uint8Array | null;
  /** True while loading credentials from IndexedDB on mount */
  isLoading: boolean;
  /** Store device credentials after signup/login */
  setDevice: (kid: string, key: Uint8Array) => void;
  /** Clear device credentials (logout) */
  clearDevice: () => void;
}

// eslint-disable-next-line @typescript-eslint/no-empty-function -- context defaults are never called
const noop = () => {};

const DeviceContext = createContext<DeviceContextValue>({
  deviceKid: null,
  privateKey: null,
  isLoading: true,
  setDevice: noop,
  clearDevice: noop,
});

async function getDb(): Promise<IDBPDatabase> {
  return openDB(DB_NAME, DB_VERSION, {
    upgrade(db) {
      if (!db.objectStoreNames.contains(STORE_NAME)) {
        db.createObjectStore(STORE_NAME);
      }
    },
  });
}

async function loadDevice(): Promise<StoredDevice | undefined> {
  const db = await getDb();
  return db.get(STORE_NAME, CURRENT_KEY) as Promise<StoredDevice | undefined>;
}

async function saveDevice(device: StoredDevice): Promise<void> {
  const db = await getDb();
  await db.put(STORE_NAME, device, CURRENT_KEY);
}

async function deleteDevice(): Promise<void> {
  const db = await getDb();
  await db.delete(STORE_NAME, CURRENT_KEY);
}

interface DeviceProviderProps {
  children: ReactNode;
}

export function DeviceProvider({ children }: DeviceProviderProps) {
  const [deviceKid, setDeviceKid] = useState<string | null>(null);
  const [privateKey, setPrivateKey] = useState<Uint8Array | null>(null);
  const [isLoading, setIsLoading] = useState(true);

  // Load from IndexedDB on mount
  useEffect(() => {
    loadDevice()
      .then((stored) => {
        if (stored) {
          setDeviceKid(stored.kid);
          setPrivateKey(stored.privateKey);
        }
      })
      .catch((err) => {
        // IndexedDB may be unavailable (private browsing, etc.)
        // Fall back to session-only mode
        console.warn('Failed to load device from IndexedDB:', err);
      })
      .finally(() => {
        setIsLoading(false);
      });
  }, []);

  const setDeviceFn = useCallback((kid: string, key: Uint8Array) => {
    setDeviceKid(kid);
    setPrivateKey(key);
    saveDevice({ kid, privateKey: key }).catch((err) => {
      console.warn('Failed to save device to IndexedDB:', err);
    });
  }, []);

  const clearDeviceFn = useCallback(() => {
    setDeviceKid(null);
    setPrivateKey(null);
    deleteDevice().catch((err) => {
      console.warn('Failed to delete device from IndexedDB:', err);
    });
  }, []);

  const value = useMemo(
    () => ({ deviceKid, privateKey, isLoading, setDevice: setDeviceFn, clearDevice: clearDeviceFn }),
    [deviceKid, privateKey, isLoading, setDeviceFn, clearDeviceFn]
  );

  return <DeviceContext.Provider value={value}>{children}</DeviceContext.Provider>;
}

/**
 * Hook to access device credentials.
 */
export function useDevice(): DeviceContextValue {
  return useContext(DeviceContext);
}
```

**Step 2: Update consumers that check `deviceKid`**

In `web/src/pages/Settings.page.tsx`, the existing code already checks `if (!deviceKid)`. Add an `isLoading` check to prevent flash of "not authenticated" UI:

```typescript
const { deviceKid, privateKey, isLoading: deviceLoading } = useDevice();

// Before the !deviceKid check:
if (deviceLoading) {
  return (
    <Stack gap="md" maw={800} mx="auto" mt="xl">
      <Title order={2}>Settings</Title>
      <Loader size="sm" />
    </Stack>
  );
}
```

**Step 3: Run tests**

Run: `cd web && yarn vitest --run --reporter=verbose`
Expected: existing tests pass. Note: if DeviceList tests or Settings tests mock `useDevice`, they may need `isLoading: false` added to the mock return value.

**Step 4: Run lint**

Run: `just lint-frontend`

**Step 5: Commit**

```bash
git add web/src/providers/DeviceProvider.tsx web/src/pages/Settings.page.tsx
git commit -m "feat(identity): Persist device credentials in IndexedDB via idb"
```

---

## Task 11: Frontend API client for login

**Files:**
- Modify: `web/src/features/identity/api/client.ts`
- Modify: `web/src/features/identity/api/queries.ts`

**Step 1: Add login types and functions to client.ts**

In `web/src/features/identity/api/client.ts`, add after the existing type definitions:

```typescript
export interface BackupResponse {
  encrypted_backup: string; // base64url
  root_kid: string;
}

export interface LoginDevice {
  pubkey: string;      // base64url
  name: string;
  certificate: string; // base64url
}

export interface LoginRequest {
  username: string;
  device: LoginDevice;
}

export interface LoginResponse {
  account_id: string;
  root_kid: string;
  device_kid: string;
}
```

Add API functions:

```typescript
export async function fetchBackup(username: string): Promise<BackupResponse> {
  return fetchJson(`/auth/backup/${encodeURIComponent(username)}`, {
    method: 'GET',
  });
}

export async function login(request: LoginRequest): Promise<LoginResponse> {
  return fetchJson('/auth/login', {
    method: 'POST',
    body: JSON.stringify(request),
  });
}
```

**Step 2: Add nonce to `buildAuthHeaders`**

In the existing `buildAuthHeaders` function, add nonce:

```typescript
async function buildAuthHeaders(
  method: string,
  path: string,
  bodyBytes: Uint8Array,
  deviceKid: string,
  privateKey: Uint8Array,
  wasmCrypto: CryptoModule
): Promise<Record<string, string>> {
  const timestamp = Math.floor(Date.now() / 1000).toString();
  const nonce = globalThis.crypto.randomUUID();
  const bodyHash = await sha256Hex(bodyBytes);
  const canonical = `${method}\n${path}\n${timestamp}\n${nonce}\n${bodyHash}`;
  const signature = ed25519.sign(new TextEncoder().encode(canonical), privateKey);

  return {
    'X-Device-Kid': deviceKid,
    'X-Signature': wasmCrypto.encode_base64url(signature),
    'X-Timestamp': timestamp,
    'X-Nonce': nonce,
  };
}
```

**Step 3: Add TanStack Query hook for login**

In `web/src/features/identity/api/queries.ts`, add:

```typescript
import {
  // ... existing imports ...
  login,
  type LoginRequest,
  type LoginResponse,
} from './client';

/**
 * Mutation hook for login
 */
export function useLogin() {
  return useMutation<LoginResponse, Error, LoginRequest>({
    mutationFn: login,
  });
}
```

**Step 4: Update client tests**

In `web/src/features/identity/api/client.test.ts`, update the test for `buildAuthHeaders` (if it exists) to check for `X-Nonce`. Also add tests for `fetchBackup` and `login`.

**Step 5: Run tests and lint**

Run: `cd web && yarn vitest --run --reporter=verbose && just lint-frontend`
Expected: all pass

**Step 6: Commit**

```bash
git add web/src/features/identity/api/client.ts \
    web/src/features/identity/api/queries.ts \
    web/src/features/identity/api/client.test.ts
git commit -m "feat(identity): Add login API client, nonce in signed requests"
```

---

## Task 12: Login page

**Files:**
- Create: `web/src/pages/Login.page.tsx`
- Modify: `web/src/Router.tsx`

**Step 1: Create Login page**

Create `web/src/pages/Login.page.tsx`:

```typescript
/**
 * Login page — recover account on a new device
 * Fetches encrypted backup, decrypts with password, authorizes device
 */

import { useState } from 'react';
import { useNavigate } from '@tanstack/react-router';
import { IconAlertTriangle } from '@tabler/icons-react';
import {
  Alert,
  Button,
  Card,
  Group,
  PasswordInput,
  Stack,
  Text,
  TextInput,
  Title,
} from '@mantine/core';
import {
  decryptBackupEnvelope,
  generateKeyPair,
  signMessage,
  fetchBackup,
  useLogin,
} from '@/features/identity';
import { useCryptoRequired } from '@/providers/CryptoProvider';
import { useDevice } from '@/providers/DeviceProvider';

export function LoginPage() {
  const crypto = useCryptoRequired();
  const loginMutation = useLogin();
  const { setDevice } = useDevice();
  const navigate = useNavigate();

  const [username, setUsername] = useState('');
  const [password, setPassword] = useState('');
  const [isDecrypting, setIsDecrypting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setError(null);

    if (!username.trim() || !password) {
      return;
    }

    setIsDecrypting(true);

    try {
      // Fetch encrypted backup
      const backupResponse = await fetchBackup(username.trim());

      // Decode and decrypt backup
      const envelopeBytes = crypto.decode_base64url(backupResponse.encrypted_backup);
      const rootPrivateKey = await decryptBackupEnvelope(envelopeBytes, password);

      // Generate new device keypair
      const deviceKeyPair = generateKeyPair(crypto);

      // Sign device certificate with root key
      const certificate = signMessage(deviceKeyPair.publicKey, rootPrivateKey);

      setIsDecrypting(false);

      // Authorize new device
      const response = await loginMutation.mutateAsync({
        username: username.trim(),
        device: {
          pubkey: crypto.encode_base64url(deviceKeyPair.publicKey),
          name: getDeviceName(),
          certificate: crypto.encode_base64url(certificate),
        },
      });

      // Store device credentials
      setDevice(response.device_kid, deviceKeyPair.privateKey);

      // Navigate to settings
      void navigate({ to: '/settings' });
    } catch (err) {
      setIsDecrypting(false);
      if (err instanceof Error) {
        // Distinguish decryption failure from API errors
        if (err.message.includes('decrypt') || err.message.includes('tag')) {
          setError('Wrong password or corrupted backup');
        } else {
          setError(err.message);
        }
      } else {
        setError('Login failed');
      }
    }
  };

  const isLoading = isDecrypting || loginMutation.isPending;

  return (
    <Stack gap="md" maw={500} mx="auto" mt="xl">
      <div>
        <Title order={2}>Log In</Title>
        <Text c="dimmed" size="sm" mt="xs">
          Recover your account on this device
        </Text>
      </div>

      <Card shadow="sm" padding="lg" radius="md" withBorder>
        <form
          onSubmit={(e) => {
            void handleSubmit(e);
          }}
        >
          <Stack gap="md">
            <TextInput
              label="Username"
              placeholder="alice"
              required
              value={username}
              onChange={(e) => {
                setUsername(e.currentTarget.value);
              }}
              disabled={isLoading}
            />

            <PasswordInput
              label="Backup Password"
              required
              value={password}
              onChange={(e) => {
                setPassword(e.currentTarget.value);
              }}
              disabled={isLoading}
            />

            {error ? (
              <Alert icon={<IconAlertTriangle size={16} />} title="Login failed" color="red">
                {error}
              </Alert>
            ) : null}

            <Group justify="flex-end">
              <Button type="submit" loading={isLoading}>
                {isDecrypting ? 'Decrypting backup...' : 'Log In'}
              </Button>
            </Group>
          </Stack>
        </form>
      </Card>

      <Text size="xs" c="dimmed" ta="center">
        Don&apos;t have an account?{' '}
        <a href="/signup">Sign up</a>
      </Text>
    </Stack>
  );
}

function getDeviceName(): string {
  const ua = navigator.userAgent;
  if (ua.includes('iPhone') || ua.includes('iPad') || ua.includes('iPod')) {
    return 'iOS Device';
  }
  if (ua.includes('Android')) {
    return 'Android Device';
  }
  if (ua.includes('Mac')) {
    return 'Mac';
  }
  if (ua.includes('Windows')) {
    return 'Windows PC';
  }
  if (ua.includes('Linux')) {
    return 'Linux';
  }
  return 'Browser';
}
```

**Step 2: Add route**

In `web/src/Router.tsx`, add import and route:

```typescript
import { LoginPage } from './pages/Login.page';

// After signupRoute:
const loginRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: 'login',
  component: LoginPage,
});

// Add to routeTree:
const routeTree = rootRoute.addChildren([
  homeRoute,
  dashboardRoute,
  conversationsRoute,
  aboutRoute,
  signupRoute,
  loginRoute,
  // ... rest
]);
```

**Step 3: Add "Already have an account?" link to SignupForm**

In `web/src/features/identity/components/SignupForm.tsx`, update the bottom text:

```typescript
<Text size="xs" c="dimmed" ta="center">
  Your keys are generated locally and never leave your device.
  {' '}Already have an account? <a href="/login">Log in</a>
</Text>
```

**Step 4: Run lint and tests**

Run: `just lint-frontend && just test-frontend`

**Step 5: Commit**

```bash
git add web/src/pages/Login.page.tsx web/src/Router.tsx \
    web/src/features/identity/components/SignupForm.tsx
git commit -m "feat(identity): Add login page with backup decryption flow"
```

---

## Task 13: Full-stack verification

**Step 1: Run all backend tests**

Run: `just test-backend`
Expected: all pass

**Step 2: Run all frontend tests**

Run: `just test-frontend`
Expected: all pass

**Step 3: Run all linting**

Run: `just lint`
Expected: clean

**Step 4: Commit any remaining fixes and push**

```bash
git push origin feature/device-key-auth-m3
```

---

## Implementation Order Summary

| Task | Description | Depends On |
|------|-------------|------------|
| 1 | Account lookup by username (repo) | — |
| 2 | GET /auth/backup/:username | Task 1 |
| 3 | POST /auth/login | Task 1 |
| 4 | Backend integration tests | Tasks 2, 3 |
| 5 | Nonce replay prevention | — |
| 6 | Replay prevention test | Task 5 |
| 7 | Install frontend deps | — |
| 8 | Real backup encryption | Task 7 |
| 9 | Update signup flow (password) | Task 8 |
| 10 | IndexedDB persistence | Task 7 |
| 11 | Login API client + nonce | Tasks 7, 5 |
| 12 | Login page | Tasks 8, 10, 11 |
| 13 | Full-stack verification | All |

**Parallelizable:** Tasks 1-6 (backend) can run in parallel with Tasks 7-8 (frontend crypto). Task 5 is independent of Tasks 1-4.
