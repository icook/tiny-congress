# BE-02 Canonical signing library

Goal: provide a single backend module for canonicalizing payloads (RFC 8785), deriving kids, and signing/verifying envelopes with Ed25519. Produce test vectors reused by frontend.

Deliverables
- `service/src/identity/crypto/` with canonicalization helpers, kid derivation, and Ed25519 sign/verify wrappers.
- Shared test vectors for canonical JSON, kid derivation, and signature verification.
- Negative tests that ensure tampering or non-canonical payloads are rejected.

Implementation plan (service)
1) Dependencies: add to `service/Cargo.toml`:
   - `ed25519-dalek` for key parsing/sign/verify.
   - `sha2` for SHA-256.
   - `base64` with URL_SAFE_NO_PAD for base64url.
   - `serde_jcs` (or `rfc8785` crate) for JSON Canonicalization Scheme. If `serde_jcs` is unmaintained, use `serde_canonical_json` or implement sorted-object canonicalization that matches RFC 8785.

2) Module layout under `service/src/identity/crypto/`:
   - `mod.rs` exporting `canonicalize_payload`, `derive_kid`, `sign_bytes`, `verify_envelope`.
   - `canonical.rs`: takes `serde_json::Value` (payload_type, payload, signer) and returns canonical bytes using RFC 8785 rules. Reject floats that are NaN/inf.
   - `kid.rs`: `derive_kid(pubkey: &[u8]) -> String` using SHA-256 then base64url without padding.
   - `ed25519.rs`: `parse_public_key(base64/hex)`, `sign(message, secret_key)`, `verify(message, signature, public_key)`; signature/base64url conversions.
   - `envelope.rs`: `SignedEnvelope` struct mapping the canonical envelope schema; `verify_envelope(envelope_json)` should canonicalize the fields and verify the signature matches `signer.kid` + public key.

3) Test vectors:
   - Create `doc/identity-testvectors/` with JSON fixtures for canonicalization and signature checks (store both canonical string and expected hash/kid/signature). Keep the directory synced to git for frontend reuse.
   - Add Rust unit tests in `service/src/identity/crypto/tests.rs` that load the fixtures and assert deterministic outputs. Include mutation tests (swap field order, tweak whitespace) that must fail verification.

4) Error handling: define `CryptoError` enum (bad canonicalization, invalid key, signature mismatch). Map to `anyhow` at call sites but keep error kinds for testing.

5) Integration hook: export crypto helpers via `pub mod crypto;` in `service/src/identity/mod.rs` so BE-01 append logic and later tickets can depend on a single implementation.

Verification
- `cd service && cargo test identity::crypto` (ensure canonicalization and sign/verify tests pass).
- Add a check in CI matrix to run these tests in `skaffold test -p ci` (follows existing repo guidance).
- Confirm the canonical bytes from Rust match the stored fixtures byte-for-byte using `diff <(cat fixture) <(cargo test -- --nocapture ...)` when debugging.
