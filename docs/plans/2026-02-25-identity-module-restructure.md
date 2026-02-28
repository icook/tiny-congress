# Identity Module Restructure

**Date:** 2026-02-25
**Branch:** `feature/device-key-auth-m1`
**Goal:** Tighten the identity module structure to act as LLM guardrails — make wrong code harder to write by encoding constraints in types and file layout.

## Changes

### 1. `Kid` newtype in `tc-crypto`

**File:** `crates/tc-crypto/src/kid.rs`

A validated newtype wrapping `String`. A `Kid` that exists is guaranteed well-formed.

**Construction paths:**
- `derive_kid(&[u8]) -> Kid` — computed from public key, always valid
- `Kid::from_str(&str) -> Result<Kid, KidError>` — validates format on construction
- `sqlx::Decode` (behind `sqlx` feature flag) — validates on DB deserialization

**Validation rules:**
- Must be valid base64url charset (`[A-Za-z0-9_-]`)
- Must be exactly 22 characters (16 bytes base64url-encoded without padding)

**Trait implementations:**
- `Display`, `AsRef<str>`, `Serialize`/`Deserialize` (serde transparent)
- `sqlx::Type`, `sqlx::Encode`, `sqlx::Decode` — behind `#[cfg(feature = "sqlx")]`
- `PartialEq`, `Eq`, `Hash`, `Clone`, `Debug`

**Feature flags in tc-crypto:**
- `ed25519` — existing, gates `verify_ed25519`
- `sqlx` — new, gates sqlx trait impls

**Service Cargo.toml:** `tc-crypto = { features = ["ed25519", "sqlx"] }`

**Ripple effects:**
- `derive_kid` return type: `String` → `Kid`
- Repo function signatures: `root_kid: &str` → `root_kid: &Kid`
- Record types: `root_kid: String` → `root_kid: Kid`
- `SignupResponse` fields: `String` → `Kid` (serializes as string via serde)

### 2. `BackupEnvelope` in `tc-crypto`

**File:** `crates/tc-crypto/src/envelope.rs`

Argon2id-only. Fixed envelope layout:

| Offset | Size | Field |
|--------|------|-------|
| 0 | 1 | version (0x01) |
| 1 | 1 | kdf_id (0x01 = argon2id) |
| 2 | 4 | m_cost (LE u32) |
| 6 | 4 | t_cost (LE u32) |
| 10 | 4 | p_cost (LE u32) |
| 14 | 16 | salt |
| 30 | 12 | nonce |
| 42 | N | ciphertext (min 48: 32 key + 16 GCM tag) |

**Minimum size:** 90 bytes. **Maximum size:** 4096 bytes.

```rust
pub struct BackupEnvelope {
    version: u8,
    salt: [u8; 16],
    raw: Vec<u8>,
}

impl BackupEnvelope {
    pub fn parse(bytes: Vec<u8>) -> Result<Self, EnvelopeError>;
    pub fn build(salt: [u8; 16], m_cost: u32, t_cost: u32, p_cost: u32,
                 nonce: [u8; 12], ciphertext: &[u8]) -> Result<Self, EnvelopeError>;
    pub fn salt(&self) -> &[u8; 16];
    pub fn version(&self) -> u8;
    pub fn as_bytes(&self) -> &[u8];
}
```

**`EnvelopeError` variants:** `TooSmall`, `TooLarge`, `UnsupportedVersion`, `UnsupportedKdf`.

No feature gate needed — pure byte parsing, WASM-compatible.

**Handler simplification:**
```rust
// Before: ~40 lines of inline byte parsing
// After:
let envelope = BackupEnvelope::parse(backup_bytes)
    .map_err(|e| bad_request(&e.to_string()))?;
```

### 3. Drop PBKDF2 support

Migration 04 is part of this PR (not yet in production) — edit directly.

**Migration 04 changes:**
- Remove `kdf_algorithm TEXT NOT NULL CHECK (...)` column
- Keep `version INTEGER NOT NULL DEFAULT 1` (for future format evolution)

**Repo changes:**
- `create_backup_with_executor`: drop `kdf_algorithm: &str` parameter
- `BackupRecord`: drop `kdf_algorithm: String` field
- `BackupRepo` trait: update signatures accordingly

**Handler changes:**
- `parse_envelope()` deleted — replaced by `BackupEnvelope::parse()`
- No KDF dispatch logic remains

### 4. Test restructure

**Principle:** Keep fast unit tests inline with the handler. Move DB-dependent tests to domain-specific files.

**Handler unit tests stay in `service/src/identity/http/mod.rs`:**
- Validation tests (empty username, invalid keys, malformed envelope, etc.)
- Use lazy pool, never touch DB
- Serve as inline documentation of the handler contract
- Update to use `BackupEnvelope::build()` instead of manual byte vectors

**New file layout:**
```
service/tests/
├── common/                       (unchanged)
├── db_tests.rs                   (migration/schema/pgmq checks only, ~180 lines)
├── http_tests.rs                 (non-identity HTTP tests — security headers, CORS, etc.)
└── identity/
    ├── mod.rs                    (declares submodules, shared imports)
    ├── repo_tests.rs             (account, backup, device_key repo DB tests)
    └── handler_tests.rs          (signup handler integration tests with real DB)
```

**What moves where:**

`db_tests.rs` → `identity/repo_tests.rs`:
- Account repo tests (create, duplicate)
- Backup repo tests (create, duplicate KID, get_by_kid, delete)
- Device key repo tests (create, duplicate KID, max devices, revoke, rename, touch)

`db_tests.rs` → `identity/handler_tests.rs`:
- `valid_signup_json()` helper
- `test_signup_handler_success`
- `test_signup_handler_duplicate_username`

`db_tests.rs` keeps:
- `test_migrations_are_valid`
- `test_schema_matches_snapshot`
- `test_pgmq_extension_available`

`http_tests.rs` changes:
- Identity-specific integration tests that are already there stay (they test HTTP routing/CORS, not identity domain logic)
- Remove any identity-specific tests that duplicate handler_tests.rs

**Test helpers:**
- `fake_backup_envelope()` (duplicated in 2 places) → replaced by `BackupEnvelope::build()`
- `AccountFactory` stays in `common/factories/` (unchanged)

## Implementation order

1. `Kid` newtype in tc-crypto (+ feature flag for sqlx)
2. `BackupEnvelope` type in tc-crypto
3. Drop PBKDF2 from migration 04 + repo layer
4. Wire `Kid` and `BackupEnvelope` into handler and repos
5. Test restructure (move files, update imports)
6. Delete dead code, run full lint + test suite

## Non-goals

- No frontend changes (tc-crypto WASM build unaffected beyond new exports)
- No new HTTP endpoints
- No changes to device key certificate format
