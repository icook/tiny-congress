# FE-01 Key management module

Goal: frontend-side key generation and storage for root and device Ed25519 keys, with signing helpers reused by signup, login, and endorsements.

Deliverables
- `web/src/features/identity/keys/` module with APIs to generate, persist, and use Ed25519 keys.
- Storage strategy for device key (IndexedDB or secure localStorage wrapper) and optional encrypted export for root key.
- Tests (Vitest) validating deterministic signing against backend vectors.

Implementation plan (web)
1) Library choice: use `@noble/ed25519` for pure-TS Ed25519 and `@noble/hashes/sha256` for kid derivation. Add base64url helper (e.g., `uint8arrays` or small util) to match backend.

2) Folder structure: create `web/src/features/identity/keys/` containing:
   - `keyStore.ts`: handles persistence (IndexedDB via `idb-keyval` or `localforage`). Store device private key encrypted with a passphrase or OS-provided WebCrypto `crypto.subtle` AES-GCM; fall back to localStorage for demo with clear warnings.
   - `rootKey.ts`: generate root key, expose `exportRootSecret()` for recovery kit, and `deriveKid(pubKey)` consistent with BE-02 fixtures (use same test vectors stored under `doc/identity-testvectors`).
   - `signer.ts`: `signEnvelope(payloadType, payload, signerMeta)` returning envelope JSON with signature using canonicalization from BE-02 vectors (implement JCS equivalent in TS).

3) API surface: export `generateDeviceKey(deviceName)`, `getDevicePublicKey()`, `signChallenge(challenge)` (used by FE-03), and `signEndorsement(payload)`.

4) Security considerations: namespace storage per account_id/device_id; do not keep root key loaded—prompt for passphrase when needed. Add banner/tooltip in UI components describing where the key is stored.

5) Tests: add `web/src/features/identity/keys/keyStore.test.ts` using Vitest + jsdom. Load backend fixtures to assert kid derivation and signature bytes match. Include negative test tampering canonical order.

Verification
- `cd web && yarn test` (runs lint/type/vitest/build) after adding deps.
- Manual: run `yarn dev`, open console, generate keys, and confirm kid matches backend fixture.
