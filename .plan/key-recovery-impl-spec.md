# Key Recovery Implementation Specification

**Related:** [ADR-006](../docs/decisions/006-webcrypto-key-recovery.md) | [ADR-007](../docs/decisions/007-zip215-verification.md) | [Signed Envelope Spec](../docs/interfaces/signed-envelope-spec.md)
**Status:** Draft
**Last updated:** 2025-12-17

## Overview

This spec details the implementation of password-encrypted server backup for root keys with WebCrypto non-extractable import, as decided in ADR-006.

### Goals

1. Users can opt-in to server-side encrypted backup of their root private key
2. Recovery flow decrypts client-side and imports into WebCrypto as non-extractable
3. Signing uses WebCrypto Ed25519 with WASM fallback for unsupported browsers
4. Canonicalization remains in Rust/WASM per signed-envelope-spec

### Non-Goals (MVP)

- WebAuthn/passkey integration
- Social recovery / secret sharing
- Multi-device sync (beyond shared backup blob)
- Hardware key support

---

## Dependencies

### Backend (Cargo.toml)

```toml
# Signing and verification (ZIP215-compliant per ADR-007)
ed25519-consensus = "2"

# Encryption (for validation/re-encryption if needed)
aes-gcm = "0.10"
argon2 = "0.5"

# WASM generation
wasm-bindgen = "0.2"
```

### Frontend (package.json)

```json
{
  "dependencies": {
    "@noble/hashes": "^2.0.1",      // existing - for PBKDF2 fallback
    "idb-keyval": "^6.2.1"          // IndexedDB wrapper for CryptoKey storage
  },
  "devDependencies": {
    "@aspect-build/aspect-argon2": "^1.0.0"  // Argon2 WASM build
  }
}
```

**Note:** Signing/verification fallback uses the Rust/WASM module (same as verification), not `@noble/curves`. This ensures ZIP215 compliance per ADR-007.

### Browser Requirements

| Feature | Chrome | Firefox | Safari | Edge |
|---------|--------|---------|--------|------|
| Ed25519 WebCrypto | 113+ | 128+ | 17+ | 113+ |
| IndexedDB CryptoKey | Yes | Yes | Yes | Yes |
| Web Workers | Yes | Yes | Yes | Yes |

---

## Database Schema

### Migration: `XX_account_backup.sql`

```sql
-- Encrypted backup storage for root keys
-- Column names aligned with ADR-006
ALTER TABLE accounts
  ADD COLUMN encrypted_backup BYTEA,
  ADD COLUMN backup_salt BYTEA,
  ADD COLUMN backup_kdf_algorithm TEXT CHECK (backup_kdf_algorithm IN ('argon2id', 'pbkdf2')),
  ADD COLUMN backup_version INTEGER DEFAULT 1,
  ADD COLUMN backup_created_at TIMESTAMPTZ;

-- Index for recovery lookups by KID
CREATE INDEX idx_accounts_backup_kid ON accounts(root_kid) WHERE encrypted_backup IS NOT NULL;

COMMENT ON COLUMN accounts.encrypted_backup IS 'AES-256-GCM ciphertext (includes nonce + tag) of root private key';
COMMENT ON COLUMN accounts.backup_kdf_algorithm IS 'KDF used: argon2id (preferred) or pbkdf2 (fallback)';
```

**Note:** Nonce is stored within `encrypted_backup` blob per the binary envelope format (see Encrypted Backup Format section). KDF params are implicit per algorithm (Argon2id: m=19456, t=2, p=1; PBKDF2: 600k iterations).

---

## API Endpoints

### POST `/api/auth/backup`

Create or update encrypted backup.

**Request:**
```json
{
  "kid": "base64url-kid",
  "encrypted_backup": "base64url-envelope",
  "salt": "base64url-salt",
  "kdf_algorithm": "argon2id",
  "version": 1
}
```

**Note:** `encrypted_backup` contains the full binary envelope (version + KDF ID + params + nonce + ciphertext). KDF params are implicit per algorithm.

**Response:** `201 Created` or `200 OK` (update)
```json
{
  "kid": "base64url-kid",
  "backup_created_at": "2025-12-17T00:00:00Z"
}
```

**Authorization:** Requires valid signed envelope proving ownership of the KID.

### GET `/api/auth/backup/:kid`

Retrieve encrypted backup for recovery.

**Response:** `200 OK`
```json
{
  "encrypted_backup": "base64url-envelope",
  "salt": "base64url-salt",
  "kdf_algorithm": "argon2id",
  "version": 1
}
```

**Response:** `404 Not Found` if no backup exists.

**Rate Limiting:** 5 requests/minute/IP. Returns `429 Too Many Requests` with `Retry-After` header.

### DELETE `/api/auth/backup/:kid`

Remove backup (requires signed envelope).

**Response:** `204 No Content`

---

## Encrypted Backup Format

### Envelope (binary, versioned)

```
+--------+--------+----------+-------+-----------+
| Version| KDF ID | KDF Params| Nonce | Ciphertext|
| 1 byte | 1 byte | variable | 12 B  | 32 B + 16 |
+--------+--------+----------+-------+-----------+
```

| Field | Size | Description |
|-------|------|-------------|
| Version | 1 byte | Format version (`0x01`) |
| KDF ID | 1 byte | `0x01` = Argon2id, `0x02` = PBKDF2 |
| KDF Params | variable | Argon2: 12 bytes (m:4, t:4, p:4). PBKDF2: 4 bytes (iterations) |
| Salt | 16 bytes | Random salt for KDF |
| Nonce | 12 bytes | AES-GCM nonce |
| Ciphertext | 48 bytes | AES-256-GCM(private_key ‖ tag) |

### Encryption Flow

```
password → KDF(password, salt, params) → 256-bit key
private_key (32 bytes) → AES-256-GCM(key, nonce) → ciphertext (48 bytes)
```

### KDF Parameters

**Argon2id (preferred):**
- Memory: 19456 KiB (19 MiB)
- Iterations: 2
- Parallelism: 1
- Output: 32 bytes

**PBKDF2-SHA256 (fallback):**
- Iterations: 600,000
- Output: 32 bytes

---

## Frontend Architecture

### Critical: Verification Must Use WASM

Per ADR-007, **all signature verification in the browser MUST use the Rust/WASM module** (`verify_ed25519`). WebCrypto's `crypto.subtle.verify()` does NOT implement ZIP215 semantics and MUST NOT be used for verification.

| Operation | Allowed | Not Allowed |
|-----------|---------|-------------|
| Signing | WebCrypto `sign()` ✅ | — |
| Signing fallback | WASM `sign_ed25519()` ✅ | — |
| Verification | WASM `verify_ed25519()` ✅ | WebCrypto `verify()` ❌ |

### File Structure

```
web/src/features/identity/
├── keys/
│   ├── crypto.ts           # Existing - key generation
│   ├── types.ts            # Existing - KeyPair interface
│   ├── webcrypto.ts        # NEW - WebCrypto Ed25519 signing only
│   ├── fallback-signer.ts  # NEW - WASM fallback for signing
│   ├── verifier.ts         # NEW - WASM-only verification (ZIP215)
│   └── feature-detect.ts   # NEW - Browser capability detection
├── recovery/
│   ├── crypto.worker.ts    # NEW - Isolated decryption worker
│   ├── backup-client.ts    # NEW - API client for backup endpoints
│   ├── indexeddb.ts        # NEW - CryptoKey persistence
│   ├── kdf.ts              # NEW - Argon2id/PBKDF2 wrapper
│   └── types.ts            # NEW - Recovery-specific types
└── components/
    ├── BackupSetup.tsx     # NEW - Backup creation UI
    └── RecoveryFlow.tsx    # NEW - Recovery UI
```

### Crypto Worker (`crypto.worker.ts`)

The worker handles all sensitive operations to prevent key exposure on main thread.

```typescript
// Message types
type WorkerRequest =
  | { type: 'decrypt'; backup: EncryptedBackup; password: string }
  | { type: 'encrypt'; privateKey: Uint8Array; password: string; kdf: KdfType };

type WorkerResponse =
  | { type: 'cryptokey'; handle: CryptoKey }  // Transferred via postMessage
  | { type: 'encrypted'; backup: EncryptedBackup }
  | { type: 'error'; message: string };

// Worker implementation
self.onmessage = async (e: MessageEvent<WorkerRequest>) => {
  try {
    if (e.data.type === 'decrypt') {
      const { backup, password } = e.data;

      // 1. Derive key from password
      const derivedKey = await deriveKey(password, backup.salt, backup.kdf, backup.kdfParams);

      // 2. Decrypt private key
      const privateKeyBytes = await decryptAesGcm(backup.ciphertext, derivedKey, backup.nonce);

      // 3. Import into WebCrypto as non-extractable
      const cryptoKey = await crypto.subtle.importKey(
        'raw',
        privateKeyBytes,
        { name: 'Ed25519' },
        false,  // extractable = false
        ['sign']
      );

      // 4. Zeroize temporary buffer
      privateKeyBytes.fill(0);

      // 5. Transfer CryptoKey to main thread
      self.postMessage({ type: 'cryptokey', handle: cryptoKey }, []);
    }
  } catch (err) {
    self.postMessage({ type: 'error', message: 'Decryption failed' });
  }
};
```

### IndexedDB Persistence (`indexeddb.ts`)

```typescript
import { get, set, del } from 'idb-keyval';

const STORE_KEY = 'tc-signing-key';

interface StoredKey {
  cryptoKey: CryptoKey;
  kid: string;
  expiresAt: number;  // Unix timestamp
}

export async function storeSigningKey(cryptoKey: CryptoKey, kid: string, ttlMs: number): Promise<void> {
  await set(STORE_KEY, {
    cryptoKey,
    kid,
    expiresAt: Date.now() + ttlMs,
  });
}

export async function getSigningKey(): Promise<StoredKey | null> {
  const stored = await get<StoredKey>(STORE_KEY);
  if (!stored) return null;
  if (Date.now() > stored.expiresAt) {
    await del(STORE_KEY);
    return null;
  }
  return stored;
}

export async function clearSigningKey(): Promise<void> {
  await del(STORE_KEY);
}
```

### Feature Detection (`feature-detect.ts`)

```typescript
export interface CryptoCapabilities {
  webCryptoEd25519: boolean;
  indexedDbCryptoKey: boolean;
  webWorkers: boolean;
}

export async function detectCapabilities(): Promise<CryptoCapabilities> {
  const capabilities: CryptoCapabilities = {
    webCryptoEd25519: false,
    indexedDbCryptoKey: true,  // Assume true, fallback gracefully
    webWorkers: typeof Worker !== 'undefined',
  };

  // Test Ed25519 support
  try {
    const keyPair = await crypto.subtle.generateKey(
      { name: 'Ed25519' },
      false,
      ['sign', 'verify']
    );
    capabilities.webCryptoEd25519 = true;
  } catch {
    capabilities.webCryptoEd25519 = false;
  }

  return capabilities;
}

export function requiresWasmFallback(caps: CryptoCapabilities): boolean {
  return !caps.webCryptoEd25519;
}
```

### Signing Interface (`webcrypto.ts`)

```typescript
export interface Signer {
  sign(message: Uint8Array): Promise<Uint8Array>;
  getPublicKey(): Promise<Uint8Array>;
  kid: string;
}

export class WebCryptoSigner implements Signer {
  constructor(
    private cryptoKey: CryptoKey,
    private publicKey: Uint8Array,
    public kid: string
  ) {}

  async sign(message: Uint8Array): Promise<Uint8Array> {
    const signature = await crypto.subtle.sign(
      { name: 'Ed25519' },
      this.cryptoKey,
      message
    );
    return new Uint8Array(signature);
  }

  async getPublicKey(): Promise<Uint8Array> {
    return this.publicKey;
  }
}

export class WasmFallbackSigner implements Signer {
  constructor(
    private privateKey: Uint8Array,  // Kept in memory (less secure)
    private publicKey: Uint8Array,
    public kid: string
  ) {}

  async sign(message: Uint8Array): Promise<Uint8Array> {
    // Uses Rust/WASM module for ZIP215 compliance (ADR-007)
    const { sign_ed25519 } = await import('@tinycongress/crypto-wasm');
    return sign_ed25519(message, this.privateKey);
  }

  async getPublicKey(): Promise<Uint8Array> {
    return this.publicKey;
  }
}
```

---

## WASM Module (Crypto)

The WASM module provides canonicalization, signing, and verification. All three use Rust to ensure ZIP215 compliance (ADR-007).

### Rust Source (`service/src/wasm/crypto.rs`)

```rust
use wasm_bindgen::prelude::*;
use ed25519_consensus::{SigningKey, VerificationKey, Signature};
use serde_json::Value;

/// Canonicalize envelope fields for signing (RFC 8785)
#[wasm_bindgen]
pub fn canonical_signing_bytes(
    payload_type: &str,
    payload_json: &str,
    signer_json: &str,
) -> Result<Vec<u8>, JsError> {
    let payload: Value = serde_json::from_str(payload_json)?;
    let signer: Value = serde_json::from_str(signer_json)?;

    let signing_obj = serde_json::json!({
        "payload_type": payload_type,
        "payload": payload,
        "signer": signer,
    });

    let canonical = json_canonicalization::serialize(&signing_obj)?;
    Ok(canonical.into_bytes())
}

/// Sign a message with Ed25519 (fallback when WebCrypto unavailable)
#[wasm_bindgen]
pub fn sign_ed25519(message: &[u8], private_key: &[u8]) -> Result<Vec<u8>, JsError> {
    let key_bytes: [u8; 32] = private_key
        .try_into()
        .map_err(|_| JsError::new("Invalid private key length"))?;
    let signing_key = SigningKey::from(key_bytes);
    let signature = signing_key.sign(message);
    Ok(signature.to_bytes().to_vec())
}

/// Verify an Ed25519 signature (ZIP215 semantics)
#[wasm_bindgen]
pub fn verify_ed25519(message: &[u8], signature: &[u8], public_key: &[u8]) -> Result<bool, JsError> {
    let sig_bytes: [u8; 64] = signature
        .try_into()
        .map_err(|_| JsError::new("Invalid signature length"))?;
    let key_bytes: [u8; 32] = public_key
        .try_into()
        .map_err(|_| JsError::new("Invalid public key length"))?;

    let sig = Signature::from(sig_bytes);
    let vk = VerificationKey::try_from(key_bytes)
        .map_err(|_| JsError::new("Invalid public key"))?;

    // ZIP215 verification
    Ok(vk.verify(&sig, message).is_ok())
}
```

### Build Configuration

```toml
# service/Cargo.toml
[lib]
crate-type = ["cdylib", "rlib"]

[dependencies]
wasm-bindgen = "0.2"
ed25519-consensus = "2"
serde_json = "1"
json-canonicalization = "0.5"

[profile.release]
opt-level = "s"
lto = true
```

### Vite Integration

```javascript
// web/vite.config.mjs
import wasm from 'vite-plugin-wasm';

export default {
  plugins: [wasm()],
  worker: {
    format: 'es',
    plugins: [wasm()],
  },
};
```

---

## Implementation Phases

### Phase 1: Foundation

1. Add database migration for backup columns
2. Implement backup API endpoints (no auth initially, add later)
3. Add `ed25519-consensus` to backend for ZIP215-compliant signing/verification
4. Create cross-environment test vectors (generate from Rust, validate in WASM)

**Deliverables:**
- Migration `XX_account_backup.sql`
- `POST/GET/DELETE /api/auth/backup/:kid` endpoints
- Test vectors in `service/tests/crypto_vectors.rs`

### Phase 2: WASM Canonicalization

1. Create WASM crate for canonicalization
2. Build with `wasm-pack`
3. Integrate into frontend build
4. Verify canonical bytes match between Rust and WASM

**Deliverables:**
- `service/src/wasm/` module
- `web/src/wasm/canonical.ts` bindings
- Integration tests

### Phase 3: Crypto Worker

1. Implement crypto worker with Argon2id/PBKDF2
2. Add AES-256-GCM encryption/decryption
3. Implement WebCrypto Ed25519 import
4. Add feature detection

**Deliverables:**
- `crypto.worker.ts`
- `kdf.ts` with Argon2/PBKDF2
- `feature-detect.ts`
- Worker unit tests

### Phase 4: Key Persistence

1. Implement IndexedDB storage for CryptoKey
2. Add TTL and expiration handling
3. Implement key lifecycle (logout, clear)

**Deliverables:**
- `indexeddb.ts`
- Session management integration
- E2E tests for persistence

### Phase 5: UI Integration

1. Build backup setup flow (during signup or settings)
2. Build recovery flow
3. Add fallback warnings for unsupported browsers
4. Add CSP headers

**Deliverables:**
- `BackupSetup.tsx`
- `RecoveryFlow.tsx`
- CSP configuration in `index.html`

### Phase 6: Hardening

1. Add rate limiting to recovery endpoint
2. Audit for timing attacks
3. Add monitoring/logging for recovery attempts
4. Security review

**Deliverables:**
- Rate limiter middleware
- Audit log events
- Security review document

---

## Testing Strategy

### Unit Tests

| Component | Coverage |
|-----------|----------|
| KDF derivation | Argon2id and PBKDF2 with known vectors |
| AES-GCM | Encrypt/decrypt round-trip |
| Canonicalization | RFC 8785 compliance |
| Feature detection | Mock various browser capabilities |

### Integration Tests

| Scenario | Description |
|----------|-------------|
| Full backup/recovery | Create backup, clear state, recover |
| Cross-browser | Verify signatures from Chrome validate in Firefox |
| Fallback path | Test WASM signer when WebCrypto unavailable |
| Rate limiting | Verify 429 after threshold |

### E2E Tests (Playwright)

```typescript
test('backup and recovery flow', async ({ page }) => {
  // 1. Sign up and create backup
  await page.goto('/signup');
  await page.fill('[name="password"]', 'test-password-123');
  await page.click('text=Create Backup');

  // 2. Clear local state (simulate new device)
  await page.evaluate(() => indexedDB.deleteDatabase('keyval-store'));

  // 3. Recover
  await page.goto('/recover');
  await page.fill('[name="kid"]', 'test-kid');
  await page.fill('[name="password"]', 'test-password-123');
  await page.click('text=Recover');

  // 4. Verify can sign
  await page.click('text=Sign Test Message');
  await expect(page.locator('.signature')).toBeVisible();
});
```

### Test Vectors

Store in `service/tests/fixtures/crypto_vectors.json`:

```json
{
  "ed25519": [
    {
      "private_key": "base64url...",
      "public_key": "base64url...",
      "kid": "base64url...",
      "message": "base64url...",
      "signature": "base64url..."
    }
  ],
  "backup": [
    {
      "password": "test-password",
      "salt": "base64url...",
      "kdf": "argon2id",
      "kdf_params": { "m": 19456, "t": 2, "p": 1 },
      "private_key": "base64url...",
      "encrypted_backup": "base64url..."
    }
  ]
}
```

---

## Security Considerations

### Threat Model

| Threat | Mitigation | Residual Risk |
|--------|------------|---------------|
| Server compromise | Keys encrypted client-side; server never sees plaintext | Attacker gets encrypted blobs, can attempt offline brute force |
| XSS | CSP, non-extractable keys | Attacker can call `sign()` while key in memory |
| Malicious extension | Non-extractable prevents export | Extension can still invoke signing |
| Weak password | Argon2id with high memory cost | User education; consider password strength meter |
| Timing attacks | Constant-time comparison for auth tags | Review crypto library implementations |

### CSP Headers

```html
<!-- web/index.html -->
<meta http-equiv="Content-Security-Policy" content="
  default-src 'self';
  script-src 'self' 'wasm-unsafe-eval';
  worker-src 'self' blob:;
  style-src 'self' 'unsafe-inline';
  connect-src 'self' https://api.tinycongress.com;
  frame-ancestors 'none';
">
```

### Audit Logging

Log recovery attempts (without sensitive data):

```rust
#[derive(Serialize)]
struct RecoveryAttemptLog {
    kid: String,
    ip_hash: String,  // Hashed for privacy
    success: bool,    // Inferred from subsequent auth
    timestamp: DateTime<Utc>,
}
```

---

## Open Questions

1. **Password strength requirements:** Minimum length? Complexity rules? Strength meter?
2. **Backup versioning:** How to handle algorithm upgrades? Force re-backup?
3. **Multi-device:** Should backup blob be device-specific or shared?
4. **Recovery phrase:** BIP39 mnemonic as alternative to password? Both?
5. **Session duration:** How long should CryptoKey persist in IndexedDB?

---

## References

- [ADR-006: WebCrypto Key Recovery](../docs/decisions/006-webcrypto-key-recovery.md)
- [Signed Envelope Spec](../docs/interfaces/signed-envelope-spec.md)
- [RFC 8785: JSON Canonicalization Scheme](https://datatracker.ietf.org/doc/html/rfc8785)
- [RFC 8032: Ed25519](https://datatracker.ietf.org/doc/html/rfc8032)
- [OWASP Password Storage Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Password_Storage_Cheat_Sheet.html)
- [WebCrypto API](https://developer.mozilla.org/en-US/docs/Web/API/Web_Crypto_API)
