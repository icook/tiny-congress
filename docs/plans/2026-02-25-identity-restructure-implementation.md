# Identity Module Restructure Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add `Kid` newtype and `BackupEnvelope` type to tc-crypto, drop PBKDF2, wire new types through repos and handler, restructure tests by domain.

**Architecture:** Extract domain types from the handler into `tc-crypto` so both server and future WASM frontend share validated types. `Kid` validates format on construction; `BackupEnvelope` owns the binary format. Repos accept/return `Kid` via `as_str()`/`from_str()` at the boundary. No sqlx dependency added to tc-crypto (stays WASM-clean).

**Tech Stack:** Rust (edition 2021), tc-crypto (shared crate), sqlx 0.8, axum 0.8, thiserror 2.

---

### Task 1: Add `Kid` newtype to tc-crypto

**Files:**
- Create: `crates/tc-crypto/src/kid.rs`
- Modify: `crates/tc-crypto/src/lib.rs`

**Step 1: Write tests for Kid**

Add to `crates/tc-crypto/src/kid.rs`:

```rust
//! Key Identifier (KID) — a validated, type-safe wrapper for key identifiers.
//!
//! A KID is `base64url(SHA-256(pubkey)[0:16])`, always exactly 22 characters
//! of the base64url alphabet `[A-Za-z0-9_-]`.

use crate::{encode_base64url, Sha256, Digest};
use std::fmt;
use std::str::FromStr;

/// A validated key identifier. Guaranteed to be 22 base64url characters.
///
/// Construct via [`Kid::derive`] (from a public key) or [`Kid::from_str`]
/// (from a string, e.g. from a database column).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Kid(String);

/// Error returned when a string is not a valid KID.
#[derive(Debug, thiserror::Error)]
#[error("invalid KID: {reason}")]
pub struct KidError {
    reason: &'static str,
}

/// Expected length of a KID string (16 bytes base64url-encoded without padding).
const KID_LENGTH: usize = 22;

impl Kid {
    /// Derive a KID from a public key.
    ///
    /// Computed as `base64url(SHA-256(pubkey)[0:16])`.
    #[must_use]
    pub fn derive(public_key: &[u8]) -> Self {
        let hash = Sha256::digest(public_key);
        Self(encode_base64url(&hash[..16]))
    }

    /// Return the KID as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Validate that a string is a well-formed KID.
    fn validate(s: &str) -> Result<(), KidError> {
        if s.len() != KID_LENGTH {
            return Err(KidError {
                reason: "must be exactly 22 characters",
            });
        }
        if !s
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        {
            return Err(KidError {
                reason: "contains invalid characters (expected base64url)",
            });
        }
        Ok(())
    }
}

impl FromStr for Kid {
    type Err = KidError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::validate(s)?;
        Ok(Self(s.to_string()))
    }
}

impl fmt::Display for Kid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for Kid {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl serde::Serialize for Kid {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

impl<'de> serde::Deserialize<'de> for Kid {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Kid::from_str(&s).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derive_produces_valid_kid() {
        let kid = Kid::derive(&[1u8; 32]);
        assert_eq!(kid.as_str().len(), KID_LENGTH);
    }

    #[test]
    fn derive_matches_legacy_derive_kid() {
        let pubkey = [1u8; 32];
        let kid = Kid::derive(&pubkey);
        let legacy = crate::derive_kid(&pubkey);
        assert_eq!(kid.as_str(), &legacy);
    }

    #[test]
    fn from_str_accepts_valid_kid() {
        let kid = Kid::derive(&[0u8; 32]);
        let parsed: Kid = kid.as_str().parse().expect("valid");
        assert_eq!(kid, parsed);
    }

    #[test]
    fn from_str_rejects_wrong_length() {
        assert!("short".parse::<Kid>().is_err());
        assert!("a".repeat(23).parse::<Kid>().is_err());
    }

    #[test]
    fn from_str_rejects_invalid_chars() {
        // 22 chars but contains '!'
        assert!("abcdefghijklmnopqrstu!".parse::<Kid>().is_err());
    }

    #[test]
    fn serde_roundtrip() {
        let kid = Kid::derive(&[42u8; 32]);
        let json = serde_json::to_string(&kid).expect("serialize");
        let parsed: Kid = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(kid, parsed);
    }

    #[test]
    fn display_matches_as_str() {
        let kid = Kid::derive(&[1u8; 32]);
        assert_eq!(format!("{kid}"), kid.as_str());
    }
}
```

**Step 2: Update lib.rs to export kid module**

In `crates/tc-crypto/src/lib.rs`, add near the top (after existing imports):

```rust
mod kid;
pub use kid::{Kid, KidError};
```

Also add `serde` to tc-crypto dependencies since Kid uses serde:

In `crates/tc-crypto/Cargo.toml`, add under `[dependencies]`:
```toml
serde = { version = "1.0", features = ["derive"] }
serde_json = { version = "1.0", optional = true }
```

And add serde_json to dev-dependencies:
```toml
[dev-dependencies]
serde_json = "1.0"
```

Wait — the test uses `serde_json` but dev-deps already has `proptest` and `wasm-bindgen-test`. Just add `serde_json = "1.0"` to dev-dependencies.

**Step 3: Run tests to verify Kid works**

Run: `cargo test --manifest-path crates/tc-crypto/Cargo.toml`
Expected: All tests pass (existing + new kid tests)

**Step 4: Commit**

```bash
git add crates/tc-crypto/
git commit -m "feat(tc-crypto): Add Kid newtype with format validation"
```

---

### Task 2: Add `BackupEnvelope` type to tc-crypto

**Files:**
- Create: `crates/tc-crypto/src/envelope.rs`
- Modify: `crates/tc-crypto/src/lib.rs`

**Step 1: Write BackupEnvelope with tests**

Create `crates/tc-crypto/src/envelope.rs`:

```rust
//! Backup envelope — binary format for encrypted root key backups.
//!
//! Argon2id-only. Fixed layout:
//!
//! | Offset | Size | Field                |
//! |--------|------|----------------------|
//! | 0      | 1    | version (0x01)       |
//! | 1      | 1    | kdf_id (0x01)        |
//! | 2      | 4    | m_cost (LE u32)      |
//! | 6      | 4    | t_cost (LE u32)      |
//! | 10     | 4    | p_cost (LE u32)      |
//! | 14     | 16   | salt                 |
//! | 30     | 12   | nonce                |
//! | 42     | N    | ciphertext (min 48)  |

use std::fmt;

/// Current envelope version.
const VERSION: u8 = 0x01;
/// KDF identifier for Argon2id.
const KDF_ARGON2ID: u8 = 0x01;
/// Fixed header size: version(1) + kdf(1) + m(4) + t(4) + p(4) + salt(16) + nonce(12) = 42
const HEADER_SIZE: usize = 42;
/// Minimum ciphertext: 32 (key) + 16 (GCM tag) = 48
const MIN_CIPHERTEXT: usize = 48;
/// Minimum total envelope size.
const MIN_ENVELOPE_SIZE: usize = HEADER_SIZE + MIN_CIPHERTEXT; // 90
/// Maximum accepted envelope size (defence-in-depth).
const MAX_ENVELOPE_SIZE: usize = 4096;
/// Offset where the 16-byte salt begins.
const SALT_OFFSET: usize = 14;

/// A parsed and validated encrypted backup envelope.
///
/// The envelope is always Argon2id version 1. Construct via [`BackupEnvelope::parse`]
/// (from raw bytes, e.g. from a client request) or [`BackupEnvelope::build`]
/// (from individual fields, e.g. in tests).
pub struct BackupEnvelope {
    salt: [u8; 16],
    version: u8,
    raw: Vec<u8>,
}

/// Errors from envelope parsing or construction.
#[derive(Debug, thiserror::Error)]
pub enum EnvelopeError {
    #[error("Encrypted backup envelope too small")]
    TooSmall,
    #[error("Encrypted backup envelope too large")]
    TooLarge,
    #[error("Unsupported backup envelope version")]
    UnsupportedVersion,
    #[error("Unsupported KDF (only Argon2id is accepted)")]
    UnsupportedKdf,
    #[error("Ciphertext too small (minimum 48 bytes)")]
    CiphertextTooSmall,
}

impl BackupEnvelope {
    /// Parse and validate a raw envelope.
    ///
    /// # Errors
    ///
    /// Returns an error if the envelope is malformed, too small/large,
    /// or uses an unsupported version/KDF.
    pub fn parse(bytes: Vec<u8>) -> Result<Self, EnvelopeError> {
        if bytes.len() < MIN_ENVELOPE_SIZE {
            return Err(EnvelopeError::TooSmall);
        }
        if bytes.len() > MAX_ENVELOPE_SIZE {
            return Err(EnvelopeError::TooLarge);
        }
        if bytes[0] != VERSION {
            return Err(EnvelopeError::UnsupportedVersion);
        }
        if bytes[1] != KDF_ARGON2ID {
            return Err(EnvelopeError::UnsupportedKdf);
        }

        let mut salt = [0u8; 16];
        salt.copy_from_slice(&bytes[SALT_OFFSET..SALT_OFFSET + 16]);

        Ok(Self {
            salt,
            version: bytes[0],
            raw: bytes,
        })
    }

    /// Build an envelope from individual fields.
    ///
    /// Useful for tests and future frontend construction.
    ///
    /// # Errors
    ///
    /// Returns `EnvelopeError::CiphertextTooSmall` if ciphertext is under 48 bytes.
    /// Returns `EnvelopeError::TooLarge` if the assembled envelope exceeds 4096 bytes.
    pub fn build(
        salt: [u8; 16],
        m_cost: u32,
        t_cost: u32,
        p_cost: u32,
        nonce: [u8; 12],
        ciphertext: &[u8],
    ) -> Result<Self, EnvelopeError> {
        if ciphertext.len() < MIN_CIPHERTEXT {
            return Err(EnvelopeError::CiphertextTooSmall);
        }
        let total = HEADER_SIZE + ciphertext.len();
        if total > MAX_ENVELOPE_SIZE {
            return Err(EnvelopeError::TooLarge);
        }

        let mut raw = Vec::with_capacity(total);
        raw.push(VERSION);
        raw.push(KDF_ARGON2ID);
        raw.extend_from_slice(&m_cost.to_le_bytes());
        raw.extend_from_slice(&t_cost.to_le_bytes());
        raw.extend_from_slice(&p_cost.to_le_bytes());
        raw.extend_from_slice(&salt);
        raw.extend_from_slice(&nonce);
        raw.extend_from_slice(ciphertext);

        Ok(Self {
            salt,
            version: VERSION,
            raw,
        })
    }

    /// The 16-byte KDF salt.
    #[must_use]
    pub fn salt(&self) -> &[u8; 16] {
        &self.salt
    }

    /// Envelope version (currently always 1).
    #[must_use]
    pub fn version(&self) -> i32 {
        i32::from(self.version)
    }

    /// The raw envelope bytes (for storage).
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.raw
    }

    /// Consume and return the raw bytes.
    #[must_use]
    pub fn into_bytes(self) -> Vec<u8> {
        self.raw
    }
}

impl fmt::Debug for BackupEnvelope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BackupEnvelope")
            .field("version", &self.version)
            .field("size", &self.raw.len())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_ciphertext() -> Vec<u8> {
        vec![0xCC; MIN_CIPHERTEXT]
    }

    #[test]
    fn build_and_parse_roundtrip() {
        let salt = [0xAA; 16];
        let nonce = [0xBB; 12];
        let ct = test_ciphertext();

        let envelope = BackupEnvelope::build(salt, 65536, 3, 1, nonce, &ct).expect("build");
        assert_eq!(envelope.salt(), &salt);
        assert_eq!(envelope.version(), 1);
        assert_eq!(envelope.as_bytes().len(), MIN_ENVELOPE_SIZE);

        // Re-parse the raw bytes
        let parsed = BackupEnvelope::parse(envelope.into_bytes()).expect("parse");
        assert_eq!(parsed.salt(), &salt);
        assert_eq!(parsed.version(), 1);
    }

    #[test]
    fn parse_rejects_too_small() {
        assert!(matches!(
            BackupEnvelope::parse(vec![0u8; 10]),
            Err(EnvelopeError::TooSmall)
        ));
    }

    #[test]
    fn parse_rejects_too_large() {
        assert!(matches!(
            BackupEnvelope::parse(vec![0u8; MAX_ENVELOPE_SIZE + 1]),
            Err(EnvelopeError::TooLarge)
        ));
    }

    #[test]
    fn parse_rejects_wrong_version() {
        let mut raw = vec![0u8; MIN_ENVELOPE_SIZE];
        raw[0] = 0x02; // bad version
        raw[1] = KDF_ARGON2ID;
        assert!(matches!(
            BackupEnvelope::parse(raw),
            Err(EnvelopeError::UnsupportedVersion)
        ));
    }

    #[test]
    fn parse_rejects_pbkdf2() {
        let mut raw = vec![0u8; MIN_ENVELOPE_SIZE];
        raw[0] = VERSION;
        raw[1] = 0x02; // PBKDF2
        assert!(matches!(
            BackupEnvelope::parse(raw),
            Err(EnvelopeError::UnsupportedKdf)
        ));
    }

    #[test]
    fn build_rejects_short_ciphertext() {
        assert!(matches!(
            BackupEnvelope::build([0; 16], 0, 0, 0, [0; 12], &[0u8; 10]),
            Err(EnvelopeError::CiphertextTooSmall)
        ));
    }

    #[test]
    fn salt_extracted_correctly() {
        let salt = [0x42; 16];
        let envelope =
            BackupEnvelope::build(salt, 1, 2, 3, [0xBB; 12], &test_ciphertext()).expect("build");
        assert_eq!(envelope.salt(), &salt);
    }
}
```

**Step 2: Update lib.rs to export envelope module**

In `crates/tc-crypto/src/lib.rs`, add:

```rust
mod envelope;
pub use envelope::{BackupEnvelope, EnvelopeError};
```

**Step 3: Run tests**

Run: `cargo test --manifest-path crates/tc-crypto/Cargo.toml`
Expected: All tests pass

**Step 4: Commit**

```bash
git add crates/tc-crypto/
git commit -m "feat(tc-crypto): Add BackupEnvelope type (Argon2id-only)"
```

---

### Task 3: Drop PBKDF2 from migration and update backup repo

**Files:**
- Modify: `service/migrations/04_account_backups.sql`
- Modify: `service/src/identity/repo/backups.rs`
- Modify: `service/src/identity/repo/mod.rs`

**Step 1: Edit migration 04 to remove kdf_algorithm column**

Replace `service/migrations/04_account_backups.sql` with:

```sql
-- Encrypted backup storage for root keys
-- Stores password-encrypted root private key blobs for account recovery.
-- The server never sees plaintext key material.
-- Envelope format is Argon2id-only (version 1).

CREATE TABLE IF NOT EXISTS account_backups (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    account_id UUID NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    kid TEXT NOT NULL,                   -- denormalized from accounts.root_kid for join-free recovery lookup
    encrypted_backup BYTEA NOT NULL,     -- binary envelope: version + KDF params + salt + nonce + AES-256-GCM ciphertext
    salt BYTEA NOT NULL,                 -- KDF salt (extracted from envelope for indexing)
    version INTEGER NOT NULL DEFAULT 1,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    CONSTRAINT uq_account_backups_account UNIQUE (account_id),
    CONSTRAINT uq_account_backups_kid UNIQUE (kid)
);
```

**Step 2: Update backup repo — remove kdf_algorithm from all signatures**

In `service/src/identity/repo/backups.rs`:

- Remove `kdf_algorithm` from `BackupRecord` struct
- Remove `kdf_algorithm` from `CreatedBackup` (no change needed — it doesn't have it)
- Remove `kdf_algorithm: &str` parameter from `BackupRepo::create` trait
- Remove `kdf_algorithm: &str` parameter from `PgBackupRepo::create` impl
- Remove `kdf_algorithm: &str` parameter from `create_backup` function
- Remove `kdf_algorithm: &str` parameter from `create_backup_with_executor`
- Update SQL INSERT to remove kdf_algorithm column and binding
- Update SQL SELECT to remove kdf_algorithm column
- Remove `kdf_algorithm` from row mapping in `get_backup_by_kid`
- Update mock to remove `_kdf_algorithm` parameter

**Step 3: Update repo/mod.rs if needed**

No changes to mod.rs exports — `BackupRecord` and `CreatedBackup` are still exported, just with fewer fields.

**Step 4: Run tests to verify compilation**

Run: `cargo check --manifest-path service/Cargo.toml`
Expected: Compilation errors in handler (expected — we'll fix in Task 5). Verify repo tests pass with:
Run: `cargo test --manifest-path service/Cargo.toml -- backup`
Expected: Compilation errors (handler references kdf_algorithm). That's OK — Task 5 wires everything.

**Step 5: Commit (may need to wait until Task 5 if compilation errors)**

This task may need to be combined with Task 5 for a compilable commit.

---

### Task 4: Wire Kid into repo layer

**Files:**
- Modify: `service/src/identity/repo/accounts.rs`
- Modify: `service/src/identity/repo/device_keys.rs`
- Modify: `service/src/identity/repo/mod.rs`
- Modify: `service/Cargo.toml` (add serde dep to tc-crypto if not already)
- Modify: `service/tests/common/factories/account.rs`

**Step 1: Update accounts.rs — Kid in signatures and records**

- `CreatedAccount.root_kid`: `String` → `Kid`
- `AccountRepo::create` param: `root_kid: &str` → `root_kid: &Kid`
- `PgAccountRepo::create`: same
- `create_account` internal fn: `root_kid: &str` → `root_kid: &Kid`
- SQL `.bind(root_kid)` → `.bind(root_kid.as_str())`
- Return: `root_kid: root_kid.to_string()` → `root_kid: root_kid.clone()`
- `create_account_with_executor`: same signature change
- Mock: update to use `Kid`
- Add `use tc_crypto::Kid;` import

**Step 2: Update device_keys.rs — Kid in signatures and records**

- `DeviceKeyRecord.device_kid`: `String` → `Kid`
- `CreatedDeviceKey.device_kid`: `String` → `Kid`
- `DeviceKeyRepo` trait methods: `device_kid: &str` → `device_kid: &Kid`
- All internal fns: same
- SQL `.bind(device_kid)` → `.bind(device_kid.as_str())`
- Row mapping: `device_kid: row.get("device_kid")` → `device_kid: row.get::<String, _>("device_kid").parse().expect("invalid KID in database")`
  - Note: this `expect` is intentional — a malformed KID in the DB is a data corruption bug, not a user error
- Mock: update to use `Kid`
- Add `use tc_crypto::Kid;`

**Step 3: Update account factory**

In `service/tests/common/factories/account.rs`:
- `generate_test_keys` return: `(String, String)` → `(String, Kid)`
- Use `Kid::derive(&pubkey)` instead of `derive_kid(&pubkey)`
- Update import: `use tc_crypto::{Kid, encode_base64url};` (remove `derive_kid`)

**Step 4: This step may fail compilation — that's expected if handler still uses old types**

Run: `cargo check --manifest-path service/Cargo.toml`
Expected: Errors in handler (still uses `String` for kids). We'll fix in Task 5.

---

### Task 5: Wire Kid + BackupEnvelope into handler

**Files:**
- Modify: `service/src/identity/http/mod.rs`
- Modify: `service/Cargo.toml` (if serde dep needed for tc-crypto)

**Step 1: Update handler imports and delete parse_envelope**

In `service/src/identity/http/mod.rs`:
- Update import line to: `use tc_crypto::{decode_base64url_native as decode_base64url, verify_ed25519, Kid, BackupEnvelope};`
- Remove `derive_kid` from imports (Kid::derive replaces it)
- Delete the entire `parse_envelope` function (lines 62-101)
- Delete `const MAX_ENVELOPE_SIZE` (moved into BackupEnvelope)

**Step 2: Update signup handler**

Replace:
```rust
let root_kid = derive_kid(&root_pubkey_arr);
```
with:
```rust
let root_kid = Kid::derive(&root_pubkey_arr);
```

Replace:
```rust
let (version, kdf_algorithm, salt) = match parse_envelope(&backup_bytes) {
    Ok(parsed) => parsed,
    Err(msg) => return bad_request(msg),
};
```
with:
```rust
let envelope = match BackupEnvelope::parse(backup_bytes) {
    Ok(e) => e,
    Err(e) => return bad_request(&e.to_string()),
};
```

Replace:
```rust
let device_kid = derive_kid(&device_pubkey_bytes);
```
with:
```rust
let device_kid = Kid::derive(&device_pubkey_bytes);
```

Update `create_backup_with_executor` call — remove `kdf_algorithm` param, use envelope methods:
```rust
if let Err(e) = create_backup_with_executor(
    &mut *tx,
    account.id,
    &root_kid,
    envelope.as_bytes(),
    envelope.salt(),
    envelope.version(),
)
.await
{
    return backup_error_response(e);
}
```

Update `create_device_key_with_executor` call — pass `&device_kid`:
```rust
let device = match create_device_key_with_executor(
    &mut *tx,
    account.id,
    &device_kid,
    &req.device.pubkey,
    device_name,
    &certificate_bytes,
)
.await
```

**Step 3: Update SignupResponse**

```rust
#[derive(Debug, Serialize, Deserialize)]
pub struct SignupResponse {
    pub account_id: Uuid,
    pub root_kid: Kid,
    pub device_kid: Kid,
}
```

**Step 4: Update handler unit tests**

In the `#[cfg(test)]` block:
- Replace `fake_backup_envelope()` with `BackupEnvelope::build`:

```rust
fn test_envelope() -> BackupEnvelope {
    BackupEnvelope::build(
        [0xAA; 16],  // salt
        65536, 3, 1, // m_cost, t_cost, p_cost
        [0xBB; 12],  // nonce
        &[0xCC; 48], // ciphertext
    )
    .expect("test envelope")
}
```

- Update `SignupBody::valid()` to use:
```rust
backup_blob: encode_base64url(test_envelope().as_bytes()),
```

- Update imports: add `use tc_crypto::BackupEnvelope;`, remove manual envelope construction

**Step 5: Verify everything compiles and tests pass**

Run: `cargo test --manifest-path service/Cargo.toml --lib`
Expected: All handler unit tests pass

**Step 6: Commit**

```bash
git add service/src/ crates/tc-crypto/
git commit -m "refactor(identity): Wire Kid and BackupEnvelope through handler and repos

- Replace bare String KIDs with validated Kid newtype
- Replace inline parse_envelope with BackupEnvelope::parse
- Drop PBKDF2 support (Argon2id-only)
- Remove kdf_algorithm from migration 04 and repo layer"
```

---

### Task 6: Test restructure — split db_tests.rs

**Files:**
- Create: `service/tests/identity/mod.rs`
- Create: `service/tests/identity/repo_tests.rs`
- Create: `service/tests/identity/handler_tests.rs`
- Modify: `service/tests/db_tests.rs` (remove moved tests)
- Modify: `service/tests/http_tests.rs` (update envelope construction)

**Step 1: Create identity test directory**

Create `service/tests/identity/mod.rs`:

```rust
//! Identity domain tests — repo layer and handler integration.

mod common {
    pub use crate::common::*;
}

mod repo_tests;
mod handler_tests;
```

Wait — integration tests in `service/tests/` each act as their own crate root. A subdirectory `identity/` needs to be a module within one of those crate roots, OR be its own crate root.

The standard Rust pattern: create `service/tests/identity.rs` as the crate root, and `service/tests/identity/` as its module directory. But that conflicts — Rust 2021 doesn't allow both `identity.rs` and `identity/` at the same level.

Instead, use `service/tests/identity/main.rs` as the crate root:

Create `service/tests/identity/main.rs`:
```rust
//! Identity domain integration tests.

mod common;

mod repo_tests;
mod handler_tests;

// Re-export common from the shared common directory.
// This uses a path attribute since `common/` is a sibling of `identity/`.
```

Actually, the standard pattern used in this repo is that each `service/tests/*.rs` file is an integration test binary, and `common/` is shared via `mod common;`. For a subdirectory, we'd need `service/tests/identity.rs` that does `mod identity_tests;` but that's awkward.

**Simpler approach:** Just create two new test files at the top level:
- `service/tests/identity_repo_tests.rs`
- `service/tests/identity_handler_tests.rs`

These follow the existing pattern (`mod common;` at the top) and are unambiguous.

**Step 1 (revised): Create identity_repo_tests.rs**

Create `service/tests/identity_repo_tests.rs` with the repo tests moved from db_tests.rs:
- `test_keys()` helper (updated to return `(String, Kid)`)
- All account repo tests (lines 90-157 of db_tests.rs)
- All backup repo tests (lines 179-299 of db_tests.rs)
- All device key repo tests (lines 301-414 of db_tests.rs)
- Use `BackupEnvelope::build()` instead of `fake_backup_envelope()`

```rust
//! Identity repo integration tests — account, backup, and device key repositories.

mod common;

use common::factories::AccountFactory;
use common::test_db::test_transaction;
use tc_crypto::{encode_base64url, BackupEnvelope, Kid};
use tc_test_macros::shared_runtime_test;
use tinycongress_api::identity::repo::{
    create_account_with_executor, create_backup_with_executor, create_device_key_with_executor,
    AccountRepoError, BackupRepoError, DeviceKeyRepoError,
};
use sqlx::query_scalar;

fn test_keys(seed: u8) -> (String, Kid) {
    let pubkey = [seed; 32];
    let root_pubkey = encode_base64url(&pubkey);
    let root_kid = Kid::derive(&pubkey);
    (root_pubkey, root_kid)
}

fn test_envelope() -> BackupEnvelope {
    BackupEnvelope::build(
        [0xAA; 16],
        65536, 3, 1,
        [0xBB; 12],
        &[0xCC; 48],
    )
    .expect("test envelope")
}

// ... (all repo tests from db_tests.rs, updated for Kid + BackupEnvelope)
```

**Step 2: Create identity_handler_tests.rs**

Create `service/tests/identity_handler_tests.rs` with the handler integration tests:
- `valid_signup_json()` helper (updated to use `BackupEnvelope::build`)
- `test_signup_handler_success`
- `test_signup_handler_duplicate_username`

```rust
//! Identity handler integration tests — signup flow with real DB.

mod common;

use axum::{
    body::{to_bytes, Body},
    http::{header::CONTENT_TYPE, Method, Request, StatusCode},
};
use common::app_builder::TestAppBuilder;
use common::test_db::isolated_db;
use ed25519_dalek::{Signer, SigningKey};
use rand::rngs::OsRng;
use tc_crypto::{encode_base64url, BackupEnvelope};
use tc_test_macros::shared_runtime_test;
use tower::ServiceExt;

fn valid_signup_json(username: &str) -> String {
    let root_signing_key = SigningKey::generate(&mut OsRng);
    let root_pubkey_bytes = root_signing_key.verifying_key().to_bytes();
    let root_pubkey = encode_base64url(&root_pubkey_bytes);

    let device_signing_key = SigningKey::generate(&mut OsRng);
    let device_pubkey_bytes = device_signing_key.verifying_key().to_bytes();
    let device_pubkey = encode_base64url(&device_pubkey_bytes);

    let certificate_sig = root_signing_key.sign(&device_pubkey_bytes);
    let certificate = encode_base64url(&certificate_sig.to_bytes());

    let envelope = BackupEnvelope::build(
        [0xAA; 16], 65536, 3, 1, [0xBB; 12], &[0xCC; 48],
    ).expect("test envelope");
    let backup_blob = encode_base64url(envelope.as_bytes());

    format!(
        r#"{{"username": "{username}", "root_pubkey": "{root_pubkey}", "backup": {{"encrypted_blob": "{backup_blob}"}}, "device": {{"pubkey": "{device_pubkey}", "name": "Test Device", "certificate": "{certificate}"}}}}"#
    )
}

// ... (test_signup_handler_success, test_signup_handler_duplicate_username)
```

**Step 3: Trim db_tests.rs**

Remove from `service/tests/db_tests.rs`:
- Lines 28-33 (`test_keys` helper)
- Lines 179-414 (backup repo tests, device key repo tests)
- Lines 416-519 (handler integration tests)
- `fake_backup_envelope()` function
- Unused imports: `create_backup_with_executor`, `create_device_key_with_executor`, `BackupRepoError`, `DeviceKeyRepoError`, `ed25519_dalek`, `rand`, `tower::ServiceExt`, `TestAppBuilder`, `isolated_db` (if no longer used here)

Keep in `db_tests.rs`:
- Migration/schema tests
- pgmq extension check
- Factory tests
- Isolated DB tests
- Concurrent transaction tests

**Step 4: Update http_tests.rs**

Replace `valid_signup_body()` to use `BackupEnvelope::build` instead of manual byte vectors.

Replace the inline envelope construction in `test_identity_signup_invalid_pubkey` similarly.

Add import: `use tc_crypto::BackupEnvelope;`

**Step 5: Update schema snapshot**

The migration change (removing `kdf_algorithm`) will cause the schema snapshot to fail. Update it:

Run: `cargo test --manifest-path service/Cargo.toml -- schema_matches_snapshot`

If it fails, accept the new snapshot:
```bash
mv service/tests/snapshots/schema_snapshot__schema_matches_snapshot.snap.new \
   service/tests/snapshots/schema_snapshot__schema_matches_snapshot.snap
```

**Step 6: Run full test suite**

Run: `just test-backend`
Expected: All tests pass

**Step 7: Run lint**

Run: `just lint-backend`
Expected: Clean

**Step 8: Commit**

```bash
git add service/
git commit -m "refactor(tests): Split identity tests by domain

Move repo tests to identity_repo_tests.rs and handler integration tests
to identity_handler_tests.rs. Replace manual envelope byte vectors with
BackupEnvelope::build(). db_tests.rs now only contains migration, schema,
and infrastructure tests."
```

---

### Task 7: Final cleanup and verification

**Files:**
- Possibly modify any files with dead code

**Step 1: Check for dead code**

Run: `cargo check --manifest-path service/Cargo.toml 2>&1 | grep "warning"`

Fix any warnings (unused imports, dead code).

**Step 2: Run full lint + test suite**

Run: `just lint && just test`
Expected: All clean

**Step 3: Verify the legacy `derive_kid` WASM function still works**

The WASM-exported `derive_kid` in lib.rs should still exist unchanged (returns `String` for JS consumers). `Kid::derive` is the native Rust path.

Run: `cargo test --manifest-path crates/tc-crypto/Cargo.toml`
Expected: Existing `derive_kid` tests still pass alongside new `Kid` tests.

**Step 4: Commit any remaining cleanup**

```bash
git add -A
git commit -m "chore: Clean up dead code after identity restructure"
```

---

## Dependency Summary

- Tasks 1-2 are independent (tc-crypto only, no service changes)
- Task 3 must happen with Task 4+5 (can't have a compilable intermediate state with half the signatures changed)
- Tasks 3+4+5 should be one compilable commit or a sequence where each compiles
- Task 6 depends on Tasks 1-5 (needs the new types available)
- Task 7 is final cleanup

**Recommended batching for compilable commits:**
1. Task 1 (Kid) — standalone commit
2. Task 2 (BackupEnvelope) — standalone commit
3. Tasks 3+4+5 combined (migration + repos + handler) — one commit, since changing the migration signature breaks compilation until the handler is updated
4. Task 6 (test restructure) — standalone commit
5. Task 7 (cleanup) — standalone commit
