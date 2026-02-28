# ADR-008: Root Key / Device Key Identity Model with Encrypted Server Backup

## Status
Accepted

## Context

TinyCongress requires a cryptographic identity system that lets users prove ownership of an account and delegate signing authority to multiple devices. The system must support key recovery without the server ever seeing plaintext key material, and the trust model must be obvious from the code — not just documented in comments.

Several tensions shaped this decision:

- **Security vs. recoverability.** A purely client-held key is unrecoverable if lost. A server-held key is a single point of compromise. We need a middle path.
- **Cross-platform consistency.** Cryptographic operations (KID derivation, encoding, verification) must produce identical results in Rust and in the browser. Divergence is a class of bugs that is hard to detect and hard to fix.
- **Type-safe invariants.** LLM-assisted development optimizes for "compiles and passes tests." The identity system handles signing keys and certificates — mistakes are exploitable, not just incorrect. We need types that make wrong usage a compile error.

This ADR documents the identity model as implemented. It supersedes the draft ADRs from [PR #182](https://github.com/icook/tiny-congress/pull/182) (WebCrypto key recovery, ZIP215 verification) which were never merged but whose design rationale informed the implementation.

## Decision

### Key hierarchy

Each account has a two-tier Ed25519 key hierarchy:

```
Root Key (cold)
  ├── encrypted backup stored on server
  ├── never used for day-to-day operations
  └── certifies Device Keys

Device Key 1 (hot, daily use)
Device Key 2 (hot, daily use)
  ...up to 10 active devices
```

- **Root key**: The master identity. An Ed25519 keypair generated client-side. The private key is encrypted with a user password (Argon2id + AES-256-GCM) and stored on the server as an opaque blob. The server never sees plaintext key material. The public key and its derived KID are stored in the `accounts` table.
- **Device keys**: Per-device Ed25519 keypairs. Each is certified by the root key (the root key signs the raw 32-byte device public key). Device keys handle daily signing; the root key stays locked in the encrypted backup.

### Key Identifier (KID)

A deterministic, validated identifier derived from any Ed25519 public key:

```
KID = base64url_no_pad(SHA-256(pubkey)[0..16])
```

This always produces a 22-character string from the base64url alphabet (`[A-Za-z0-9_-]`). The `Kid` newtype in `tc-crypto` enforces these invariants — it cannot be constructed without going through `Kid::derive()` or `Kid::from_str()` (which validates). Functions accept `&Kid` instead of `&str`, making it a compile error to pass an unvalidated string.

Both root and device public keys have KIDs. The root KID is the account's primary identifier for backup recovery (denormalized into `account_backups.kid` for join-free lookup). Device KIDs are globally unique (DB constraint), which prevents certificate replay.

### Encrypted backup envelope

The `BackupEnvelope` is a versioned binary format stored as `BYTEA` in PostgreSQL:

| Offset | Size | Field | Constraint |
|--------|------|-------|------------|
| 0 | 1 | version | Must be `0x01` |
| 1 | 1 | KDF ID | Must be `0x01` (Argon2id) |
| 2 | 4 | m_cost (LE u32) | >= 65,536 (64 MiB) |
| 6 | 4 | t_cost (LE u32) | >= 3 |
| 10 | 4 | p_cost (LE u32) | >= 1 |
| 14 | 16 | salt | |
| 30 | 12 | nonce | |
| 42 | N | ciphertext | >= 48 bytes (32-byte key + 16-byte GCM tag) |

Total size must be in `[90, 4096]` bytes.

`BackupEnvelope` can only be constructed through `parse()` (from raw bytes) or `build()` (from individual fields), both of which validate all constraints. There is no public constructor. Its `Debug` impl intentionally omits raw bytes to prevent accidental logging of encrypted key material.

**The server validates KDF parameters even though it never decrypts.** This prevents a weak or buggy client from silently storing an easily-brute-forced backup. The Argon2id minimums (m_cost >= 65,536, t_cost >= 3) follow OWASP 2024 recommendations.

### Device key certificates

At signup, the client signs the raw 32-byte device public key with the root private key and submits the 64-byte Ed25519 signature as the certificate. The server verifies:

```
verify_strict(root_pubkey, device_pubkey_bytes, certificate_sig)
```

The signed message is the raw device pubkey bytes (not a structured message). This is sufficient because device KIDs are globally unique — the DB `UNIQUE` constraint on `device_keys.device_kid` prevents a certificate from being replayed for a different device. If a future "rotate device key" feature reuses key material, the message format must be extended with account binding or a nonce.

`verify_strict` (via `ed25519-dalek`) is used rather than permissive `verify`. Strict verification rejects malleable signatures, preventing signature malleability attacks.

### Atomic signup

Account creation, backup storage, and first device key registration happen in a single PostgreSQL transaction:

1. `INSERT INTO accounts` (username, root_pubkey, root_kid)
2. `INSERT INTO account_backups` (encrypted envelope, salt, version)
3. `SELECT ... FOR UPDATE` on accounts row + `INSERT INTO device_keys` (with device limit check)
4. `COMMIT`

If any step fails, the entire transaction rolls back. There is no state where an account exists without a backup or initial device key.

### Device limit and concurrency

Each account is limited to 10 active (non-revoked) device keys. The limit is enforced atomically using `SELECT ... FOR UPDATE` on the accounts row to serialize concurrent device additions, followed by an INSERT with a subquery count check:

```sql
INSERT INTO device_keys (...)
SELECT ...
WHERE (SELECT COUNT(*) FROM device_keys
       WHERE account_id = $1 AND revoked_at IS NULL) < 10
```

Without the `FOR UPDATE` lock, two concurrent requests under `READ COMMITTED` isolation could both read `count < 10`, both insert, and exceed the limit.

### Boundary validation and type safety

All validation happens at the service layer boundary (in `DefaultIdentityService::signup`). Once data passes validation and is packed into `ValidatedSignup`, the repository layer trusts the types:

- `ValidatedSignup` fields are `pub(crate)` — only code inside the crate can construct it, and only the service layer does so after full validation. Test code has a separate `#[cfg(test)]` constructor.
- Base64url decoding, byte-length enforcement (`[u8; 32]` for pubkeys, `[u8; 64]` for signatures), envelope parsing, and certificate verification all happen before the repo is called.
- Database errors that reach HTTP responses are sanitized — a test (`test_signup_internal_error_returns_safe_500`) explicitly verifies that connection strings and passwords are never leaked.

### Cross-language consistency

The `tc-crypto` crate compiles to both native Rust (for the backend) and WASM (for the browser), per [ADR-006](006-wasm-crypto-sharing.md). KID derivation, base64url encoding/decoding, and envelope parsing use the same Rust code on both platforms. A known test vector (`[1u8; 32]` -> `"cs1uhCLEB_ttCYaQ8RMLfQ"`) is asserted in both Rust and TypeScript test suites.

Ed25519 signature verification is only compiled into the native build (`ed25519` feature flag) — the server verifies signatures, not the client.

## Consequences

### Positive
- The root key never exists in plaintext on the server. Recovery requires the user's password.
- Device keys can be individually revoked without rotating the root key.
- The `Kid` and `BackupEnvelope` newtypes make it structurally impossible to pass unvalidated data to the database layer.
- `pub(crate)` on `ValidatedSignup` enforces that validation cannot be skipped, even by code inside the same crate but outside the identity module.
- A single shared WASM module eliminates cross-language cryptographic divergence.
- Atomic signup prevents partial account state.

### Negative
- The root key exists briefly in plaintext in the client's memory during recovery. `extractable: false` on the WebCrypto import reduces accidental leakage but does not protect against XSS.
- Only Argon2id is supported. No KDF dispatch logic exists. Adding a second KDF requires a new envelope version and new parsing code.
- The certificate message format (raw pubkey bytes) is minimal. Extending it for key rotation requires careful migration of existing certificates — or accepting that old certificates use the simple format.
- The 10-device limit is a hard constant (`MAX_DEVICES_PER_ACCOUNT`), not configurable per account.

### Neutral
- The backup format is versioned (`0x01`), allowing future format changes without breaking existing backups.
- `salt` is extracted from the envelope and stored in a separate DB column for potential future indexing, even though no current query uses it.
- Device keys have `revoked_at` and `last_used_at` timestamp columns, supporting future audit and lifecycle features.

## Alternatives considered

### Store root key in WebCrypto only (no server backup)
- Stronger security — key never leaves the device
- Unrecoverable if device is lost, which is unacceptable for most users
- Rejected for MVP; may be offered as an opt-in mode for advanced users

### Duplicate crypto implementations in TypeScript and Rust
- Eliminated by ADR-006 (WASM crypto sharing)
- High risk of subtle divergence in encoding, padding, or verification semantics

### Use a structured/canonical certificate message instead of raw pubkey bytes
- Considered adding `account_id || device_pubkey || nonce` as the signed message
- Rejected because the DB uniqueness constraint on `device_kid` already prevents replay
- Simpler format is easier to verify and harder to get wrong
- Can be extended later if key rotation requires it

### WebAuthn/passkeys for device key management
- Preferred long-term direction for device-bound keys
- Out of scope for MVP due to implementation complexity and browser compatibility gaps
- The current device key model can coexist with WebAuthn-backed keys in the future

### Multiple KDF algorithms from day one
- PR #182's draft spec included both Argon2id and PBKDF2
- The implementation accepts only Argon2id (`kdf_id = 0x01`) — no dispatch logic for a second algorithm
- Follows the "don't ship dead code paths" principle: add PBKDF2 when a browser-compatibility need arises, not before

## References
- [ADR-006: Share Crypto Code via WASM](006-wasm-crypto-sharing.md) — the shared `tc-crypto` crate this model depends on
- [PR #182: WebCrypto key recovery draft](https://github.com/icook/tiny-congress/pull/182) — original design exploration (open, not merged)
- [PR #279: Device key auth backend](https://github.com/icook/tiny-congress/pull/279) — implementation that landed
- [PR #329: Consolidate identity traits](https://github.com/icook/tiny-congress/pull/329) — refactor to current IdentityService/IdentityRepo architecture
- `crates/tc-crypto/src/` — Kid, BackupEnvelope, verify_ed25519 implementations
- `service/src/identity/` — service, repo, and HTTP handler layers
- `service/migrations/03_accounts.sql`, `05_account_backups.sql`, `06_device_keys.sql` — schema
