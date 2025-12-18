# ADR-006: Password-encrypted server backup of root key with WebCrypto non-extractable import

**Date:** 2025-12-17
**Status:** Draft
**Decision owners:** TinyCongress core team

## Context

We maintain canonical cryptographic rules in Rust. The web UI must sign and verify Ed25519 messages using the same message encoding as the backend.

For MVP recovery, we want a convenience feature: the user can store the root private key on the server encrypted under a user password. During recovery the client fetches the encrypted blob, derives a key from the password, decrypts, and restores the root key for signing.

Concern: minimize subtle crypto implementation bugs and reduce key handling risks in the web client.

### Current state (as of 2025-12-17)

* Frontend uses `@noble/curves` for Ed25519 key generation only. No signing implementation exists.
* Backend has SHA-256 for KID derivation but no Ed25519 signing library.
* No WASM crypto modules exist. Canonicalization and signing are not yet implemented.
* Keys are generated in component state and lost on unmount. No persistence or backup.
* Database stores only public key and KID. No encrypted backup storage.
* No CSP headers configured.

## Decision

1. **Canonical encoding and verification rules remain defined in Rust.**

   * Browser uses Rust-compiled-to-WASM (or a generated codec) for message canonicalization and any consensus-critical verification rules.
2. **Client-side signing uses WebCrypto Ed25519 when available.**

   * On recovery, after decrypting the root key, the client **imports it into WebCrypto as a `CryptoKey` with `extractable: false` and `keyUsages: ["sign"]`** (not `["sign", "verify"]`) and uses `subtle.sign()` for signing.
   * **Browser requirements:** Ed25519 in WebCrypto requires Chrome 113+, Edge 113+, Safari 17+, Firefox 128+. At time of writing this covers ~92% of global browser usage.
   * **Fallback strategy:** On unsupported browsers, signing falls back to the Rust/WASM module (same module used for verification). This path loses the `extractable: false` benefit but maintains functional parity. The UI should display a warning indicating reduced key isolation.
3. **We treat `extractable: false` as footgun reduction, not a hard security boundary.**

   * It prevents accidental export via WebCrypto APIs and discourages app-level serialization of key bytes.
   * It does not protect against XSS, malicious extensions, or malicious code running in-origin (which can still call `sign()`).
4. **Backup blob lifecycle:**

   * The server stores only the encrypted key blob and associated metadata (KDF params, salt, version).
   * The client does not re-export the private key from WebCrypto. If re-backup is needed (e.g., password change), the user must re-enter their recovery phrase/seed to derive the key material again. The original encrypted blob can be re-used if only extending to additional devices.

## Consequences

**Positive**

* Reduces accidental leakage risks: no exporting, no logging, no app state persistence of decrypted bytes after import.
* Uses well-maintained browser crypto primitives for signing.
* Keeps protocol correctness centralized in Rust (canonical bytes-to-sign, verification semantics).

**Negative / Limitations**

* Decrypted key material exists in client memory at least briefly during recovery import. This remains vulnerable to XSS and hostile extensions.
* Non-extractable keys can still be abused by any code with access to the `CryptoKey` handle to sign arbitrary messages.
* Requires browser support for Ed25519 WebCrypto (~92% coverage). Fallback to WASM signing loses `extractable: false` benefit.
* Worker isolation requirement means `CryptoKey` handle management adds complexity (transferability, lifetime across worker restarts).
* Significant implementation work required: WASM module for canonicalization, Web Worker, IndexedDB persistence layer, new API endpoints, database migration.

## Alternatives considered

1. **Duplicate crypto in TypeScript and Rust.** Rejected due to high risk of subtle divergence in encoding/validation and library semantics.
2. **Rust/WASM for all signing and verification.** Not preferred for private key hygiene because keys live in JS/WASM memory and cannot be made meaningfully non-extractable.
3. **No server backup.** Stronger security but worse UX and recovery.
4. **WebAuthn/passkeys (device-bound keys) for recovery/signing.** Preferred future direction but out of scope for MVP.
5. **Social recovery / secret sharing.** Also future work, more complexity for MVP.

## Implementation notes

* **Encryption format:** AES-256-GCM with a versioned envelope containing KDF parameters, salt, and nonce.
* **KDF:** Argon2id (preferred) with OWASP-recommended parameters (m=19456 KiB, t=2, p=1). PBKDF2-SHA256 with 600,000 iterations as fallback where Argon2 is unavailable.
* **Worker isolation (required):** Decryption and key import MUST occur in a dedicated Web Worker. The plaintext key bytes never transit to the main thread. The Worker imports directly into WebCrypto and returns only the `CryptoKey` handle (which is transferable via `postMessage`).
* Best-effort zeroization of temporary buffers after import (TypedArray overwrite).
* **Decryption failure handling:**
  * Wrong password → AES-GCM authentication tag fails → generic "incorrect password or corrupted backup" error (no distinction to avoid oracle attacks).
  * Corrupted blob → same error message. Client may offer retry or manual recovery phrase entry.
* Strict separation between:

  * **Canonicalization** (Rust/WASM)
  * **Signing** (WebCrypto when available, WASM fallback)
  * **Verification** (Rust/WASM only — per [ADR-007](007-zip215-verification.md), ZIP215 semantics require WASM; WebCrypto `verify()` cannot be used)

* **CryptoKey session persistence:** After import, the `CryptoKey` handle is stored in IndexedDB (which supports structured clone of non-extractable keys). On page reload, retrieve from IndexedDB rather than re-decrypting.

* **Key lifecycle:**
  * Logout: Clear `CryptoKey` from IndexedDB and Worker memory.
  * Tab close: `CryptoKey` persists in IndexedDB until explicit logout or TTL expiry.
  * Session expiry: IndexedDB entry has TTL; expired keys require re-recovery.
  * Worker restart: Retrieve `CryptoKey` from IndexedDB; no re-decryption needed.

* **Server-side rate limiting:** Recovery endpoint (`GET /auth/backup/:kid`) rate-limited to 5 attempts per minute per IP. Failed decryption attempts (inferred from lack of subsequent authenticated request) may trigger progressive delays.

* **Database schema extension:**
  ```sql
  ALTER TABLE accounts ADD COLUMN encrypted_backup BYTEA;
  ALTER TABLE accounts ADD COLUMN backup_kdf_algorithm TEXT;  -- 'argon2id' or 'pbkdf2'
  ALTER TABLE accounts ADD COLUMN backup_salt BYTEA;
  ALTER TABLE accounts ADD COLUMN backup_version INTEGER DEFAULT 1;
  ALTER TABLE accounts ADD COLUMN backup_created_at TIMESTAMPTZ;
  ```

## Related decisions

* [ADR-007: ZIP215 verification](007-zip215-verification.md) — All verification uses ZIP215 semantics via WASM.
* [Signed Envelope Spec](../interfaces/signed-envelope-spec.md) — Defines envelope structure and canonicalization.

## Follow-ups

* Add cross-environment test vectors: canonical payload bytes and known signatures.
* Define XSS hardening baseline (CSP, dependency controls) since client-side recovery inherently raises stakes.
* Evaluate WebAuthn-based recovery/signing post-MVP.
* Define canonical message format (bytes-to-sign structure) for Ed25519 signatures.
* Add backend dependencies: `ed25519-consensus` (for ZIP215 verification), `aes-gcm`, `argon2` crates.
* Build WASM crypto module with `wasm-pack` for canonicalization.
* Implement browser feature detection for Ed25519 WebCrypto support.
* Add Vite worker build configuration for dedicated crypto Worker.
