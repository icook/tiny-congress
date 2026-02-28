# M3: Login Flow + Real Backup Encryption

## Context

M2 (PR #317) delivered device management endpoints, signed-header authentication, and a minimal Settings page. However, the system only supports signup — there is no way to log in on a new device or recover after clearing browser data. The backup envelope stores random bytes instead of encrypted key material (#319), and signed requests have no replay protection (#318).

M3 makes the identity system usable end-to-end: real encryption, login, device persistence, and replay prevention.

**Branch:** `feature/device-key-auth-m3` off `master`

## Login Flow

```
User enters username + password
        |
        v
GET /auth/backup/:username     (new, unauthenticated)
  Returns encrypted backup blob
        |
        v
Client decrypts backup:
  Argon2id(password, salt, params) -> 32-byte key
  ChaCha20-Poly1305(key, nonce, ciphertext) -> root_private_key
        |
        v
Client generates device keypair
Client signs certificate: ed25519.sign(device_pubkey, root_private_key)
        |
        v
POST /auth/login               (new, unauthenticated)
  { username, device: { pubkey, name, certificate } }
  Server verifies cert against stored root_pubkey
  Server creates device key
        |
        v
Client stores device credentials in IndexedDB
Redirect to Settings page (authenticated)
```

## Decisions

**Cipher:** ChaCha20-Poly1305 (not XChaCha20). The existing BackupEnvelope format uses a 12-byte nonce field, which fits ChaCha20 exactly. No format migration needed.

**KDF:** Argon2id with m=65536 (64 MiB), t=3, p=1. Matches the minimum enforced by BackupEnvelope::parse on the server. Takes ~1s in WASM on modern hardware.

**Key storage:** IndexedDB via the `idb` package. Stores raw Ed25519 private key bytes (Uint8Array). The DeviceProvider loads from IndexedDB on mount and writes on setDevice/clearDevice.

**Replay prevention:** In-memory `HashMap<String, Instant>` for seen nonces with periodic cleanup. Nonces are UUIDs included in the canonical signed message. Sufficient for single-process deployment; a distributed store can replace the HashMap later without protocol changes.

**Frontend crypto deps:** `hash-wasm` for Argon2id (WASM-powered, lightweight), `@noble/ciphers` for ChaCha20-Poly1305 (same author as `@noble/curves` already in the project).

## Backend Changes

### GET /auth/backup/:username

Unauthenticated endpoint for the login flow. Looks up account by username, fetches the associated backup.

```rust
#[derive(Serialize)]
pub struct BackupResponse {
    pub encrypted_backup: String,  // base64url of raw envelope bytes
    pub root_kid: String,
}

pub async fn get_backup(
    Extension(pool): Extension<PgPool>,
    Path(username): Path<String>,
) -> impl IntoResponse
```

Returns 404 if account or backup not found. Rate limiting deferred to M4 (tracked in existing issues).

Requires new repo function: `get_account_by_username(executor, username) -> Result<AccountRecord, AccountRepoError>`.

### POST /auth/login

Unauthenticated endpoint that creates a device key using a root-key-signed certificate. Structurally similar to `POST /auth/devices` (add_device) but authenticates via certificate verification against the stored root pubkey instead of requiring an existing authenticated device.

```rust
#[derive(Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub device: LoginDevice,
}

#[derive(Deserialize)]
pub struct LoginDevice {
    pub pubkey: String,       // base64url Ed25519 pubkey
    pub name: String,
    pub certificate: String,  // base64url Ed25519 signature
}

#[derive(Serialize)]
pub struct LoginResponse {
    pub account_id: Uuid,
    pub root_kid: Kid,
    pub device_kid: Kid,
}

pub async fn login(
    Extension(pool): Extension<PgPool>,
    Json(req): Json<LoginRequest>,
) -> impl IntoResponse
```

Validation mirrors `validate_add_device_request`: pubkey 32 bytes, name 1-128 chars, certificate 64 bytes, verify cert against account's root_pubkey.

### Replay Prevention (Nonce Store)

Add `X-Nonce` header to the signed request scheme.

New canonical message format:
```
{METHOD}\n{PATH_AND_QUERY}\n{TIMESTAMP}\n{NONCE}\n{BODY_SHA256_HEX}
```

```rust
pub struct NonceStore {
    seen: RwLock<HashMap<String, Instant>>,
}

impl NonceStore {
    pub fn check_and_insert(&self, nonce: &str) -> bool { ... }
    pub fn cleanup(&self) { ... }  // remove entries older than 2 * MAX_TIMESTAMP_SKEW
}
```

The NonceStore is added as an Axum Extension. The AuthenticatedDevice extractor reads `X-Nonce`, rejects duplicates, and includes the nonce in the canonical message before signature verification.

Cleanup runs on a background interval (tokio::spawn + tokio::time::interval), or lazily on each request if the map exceeds a threshold.

### Router Updates

```rust
.route("/auth/backup/:username", get(get_backup))
.route("/auth/login", post(login))
```

## Frontend Changes

### Real Backup Encryption (crypto.ts)

Replace the placeholder `buildBackupEnvelope` with actual encryption:

```typescript
async function buildBackupEnvelope(
    rootPrivateKey: Uint8Array,
    password: string
): Promise<Uint8Array>
```

1. Generate 16-byte random salt, 12-byte random nonce
2. Argon2id(password, salt, m=65536, t=3, p=1) -> 32-byte key
3. ChaCha20-Poly1305.encrypt(key, nonce, rootPrivateKey) -> ciphertext (32 + 16 = 48 bytes)
4. Assemble envelope: `[version:1][kdf:1][m:4LE][t:4LE][p:4LE][salt:16][nonce:12][ciphertext:48]`

Add the inverse:

```typescript
async function decryptBackupEnvelope(
    envelope: Uint8Array,
    password: string
): Promise<Uint8Array>
```

1. Parse header: version, kdf, m/t/p, salt, nonce
2. Argon2id(password, salt, m, t, p) -> 32-byte key
3. ChaCha20-Poly1305.decrypt(key, nonce, ciphertext) -> rootPrivateKey
4. Validate result is 32 bytes

### Login Page (Login.page.tsx)

New page at `/login`:
- Username + password form (reuse Mantine form components)
- On submit: call `fetchBackup(username)` -> `decryptBackupEnvelope(blob, password)` -> generate device -> `login(request)` -> `setDevice(kid, key)`
- Loading state during Argon2id (~1s) with descriptive message
- Error handling: wrong password (decryption failure), account not found, device creation failure
- Link to signup page and vice versa

### IndexedDB Persistence (DeviceProvider.tsx)

Update DeviceProvider to persist device credentials across page reloads:

```typescript
// idb store: "tc-device-store", object store: "device"
// Key: "current", Value: { kid: string, privateKey: Uint8Array }

interface DeviceContextValue {
    deviceKid: string | null;
    privateKey: Uint8Array | null;
    isLoading: boolean;          // true while reading from IndexedDB on mount
    setDevice: (kid: string, key: Uint8Array) => void;
    clearDevice: () => void;
}
```

- On mount: read from IndexedDB, set state if found
- `setDevice`: write to IndexedDB + update state
- `clearDevice`: delete from IndexedDB + clear state
- `isLoading` prevents flash of unauthenticated UI

### Replay Prevention (client.ts)

Update `buildAuthHeaders` to include `X-Nonce`:

```typescript
async function buildAuthHeaders(...): Promise<Record<string, string>> {
    const nonce = crypto.randomUUID();
    const canonical = `${method}\n${path}\n${timestamp}\n${nonce}\n${bodyHash}`;
    // ...
    return {
        'X-Device-Kid': deviceKid,
        'X-Signature': wasmCrypto.encode_base64url(signature),
        'X-Timestamp': timestamp,
        'X-Nonce': nonce,
    };
}
```

### Update Signup Flow

- `buildBackupEnvelope` now takes `(rootPrivateKey, password)` — thread password from SignupForm
- Store root_kid in DeviceProvider or localStorage for potential re-login hint
- After signup success: device is persisted to IndexedDB via setDevice (already called)

## New Dependencies

| Package | Purpose | Size |
|---------|---------|------|
| `hash-wasm` | Argon2id KDF (WASM-powered) | ~45 KB |
| `@noble/ciphers` | ChaCha20-Poly1305 AEAD | ~20 KB |
| `idb` | IndexedDB Promise wrapper | ~3 KB |

## Testing

**Backend integration tests:**
- GET /auth/backup/:username — success, not found
- POST /auth/login — success, invalid cert, unknown username
- Nonce replay rejection (same nonce within window)
- Nonce accepted after cleanup/expiry

**Frontend unit tests:**
- buildBackupEnvelope + decryptBackupEnvelope roundtrip
- decryptBackupEnvelope with wrong password fails
- DeviceProvider IndexedDB persistence (mock idb)
- Login page form submission flow
- buildAuthHeaders includes X-Nonce

## Open Questions

None — all key decisions resolved during design.

## Closes

- #318 (replay attack prevention)
- #319 (real backup encryption)
