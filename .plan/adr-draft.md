# ADR-00X: Password-encrypted server backup of root key with WebCrypto non-extractable import

**Date:** 2025-12-17
**Status:** Draft
**Decision owners:** TinyCongress core team

## Context

We maintain canonical cryptographic rules in Rust. The web UI must sign and verify Ed25519 messages using the same message encoding as the backend.

For MVP recovery, we want a convenience feature: the user can store the root private key on the server encrypted under a user password. During recovery the client fetches the encrypted blob, derives a key from the password, decrypts, and restores the root key for signing.

Concern: minimize subtle crypto implementation bugs and reduce key handling risks in the web client.

## Decision

1. **Canonical encoding and verification rules remain defined in Rust.**

   * Browser uses Rust-compiled-to-WASM (or a generated codec) for message canonicalization and any consensus-critical verification rules.
2. **Client-side signing uses WebCrypto Ed25519 when available.**

   * On recovery, after decrypting the root key, the client **imports it into WebCrypto as a `CryptoKey` with `extractable: false`** and uses `subtle.sign()` for signing.
3. **We treat `extractable: false` as footgun reduction, not a hard security boundary.**

   * It prevents accidental export via WebCrypto APIs and discourages app-level serialization of key bytes.
   * It does not protect against XSS, malicious extensions, or malicious code running in-origin (which can still call `sign()`).
4. **Backup blob lifecycle:**

   * The server stores only the encrypted key blob and associated metadata (KDF params, salt, version).
   * The client does not re-export the private key from WebCrypto. If re-backup is needed, we re-use the original encrypted blob or require an explicit “re-encrypt from plaintext” flow that re-derives from the password (accepting temporary plaintext exposure during that operation).

## Consequences

**Positive**

* Reduces accidental leakage risks: no exporting, no logging, no app state persistence of decrypted bytes after import.
* Uses well-maintained browser crypto primitives for signing.
* Keeps protocol correctness centralized in Rust (canonical bytes-to-sign, verification semantics).

**Negative / Limitations**

* Decrypted key material exists in client memory at least briefly during recovery import. This remains vulnerable to XSS and hostile extensions.
* Non-extractable keys can still be abused by any code with access to the `CryptoKey` handle to sign arbitrary messages.
* Requires browser support for Ed25519 WebCrypto. A fallback path (WASM signing or alternate key type) may be needed.

## Alternatives considered

1. **Duplicate crypto in TypeScript and Rust.** Rejected due to high risk of subtle divergence in encoding/validation and library semantics.
2. **Rust/WASM for all signing and verification.** Not preferred for private key hygiene because keys live in JS/WASM memory and cannot be made meaningfully non-extractable.
3. **No server backup.** Stronger security but worse UX and recovery.
4. **WebAuthn/passkeys (device-bound keys) for recovery/signing.** Preferred future direction but out of scope for MVP.
5. **Social recovery / secret sharing.** Also future work, more complexity for MVP.

## Implementation notes

* Encrypted backup format is versioned and includes KDF parameters and salt.
* Client performs decrypt in an isolated execution context when feasible (dedicated worker) and avoids storing plaintext bytes in app state.
* Best-effort zeroization of temporary buffers after import.
* Strict separation between:

  * **Canonicalization** (Rust/WASM)
  * **Signing** (WebCrypto)
  * **Verification semantics** (Rust canonical, optionally mirrored in UI via WASM)

## Follow-ups

* Add cross-environment test vectors: canonical payload bytes and known signatures.
* Define XSS hardening baseline (CSP, dependency controls) since client-side recovery inherently raises stakes.
* Evaluate WebAuthn-based recovery/signing post-MVP.
