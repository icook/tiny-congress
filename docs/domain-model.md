# Domain Model

TinyCongress is a community governance platform built around cryptographic identity. Users generate Ed25519 key pairs client-side; the server never sees private key material. The trust model is: **the server is a dumb witness, not a trusted authority.**

This document is the canonical reference for domain concepts, data invariants, and trust boundaries. It's meant to be read before writing code that touches identity, cryptography, or account management.

## Entities

### Account

An account is the root identity in the system. It's anchored to a username and a root Ed25519 public key.

| Field | Type | Constraint |
|-------|------|------------|
| `id` | UUID | PK, generated |
| `username` | TEXT | Unique, 3–64 chars, `[a-zA-Z0-9_-]` |
| `root_pubkey` | TEXT | Base64url-encoded 32-byte Ed25519 public key |
| `root_kid` | TEXT | Unique, derived KID (see [Key Identifier](#key-identifier-kid)) |
| `created_at` | TIMESTAMPTZ | Immutable |

The root key is the highest-privilege credential. It's meant for cold storage — used only to delegate device keys and (future) sign recovery policies. Day-to-day operations use device keys instead.

**Not yet built:** GDPR account deletion. No code exists — don't scaffold prematurely.

**Username rules:**
- 3–64 characters, ASCII `[a-zA-Z0-9_-]` only
- Trimmed before validation
- Case-insensitive match against reserved list: `admin`, `administrator`, `root`, `system`, `mod`, `moderator`, `support`, `help`, `api`, `graphql`, `auth`, `signup`, `login`, `null`, `undefined`, `anonymous`

### Device Key

A delegated Ed25519 key for daily use. The root key signs a certificate over the device key to prove authorization.

| Field | Type | Constraint |
|-------|------|------------|
| `id` | UUID | PK, generated |
| `account_id` | UUID | FK → `accounts`, indexed |
| `device_kid` | TEXT | Globally unique KID |
| `device_pubkey` | TEXT | Base64url 32-byte Ed25519 public key |
| `device_name` | TEXT | 1–128 chars, user-provided |
| `certificate` | BYTEA | 64-byte Ed25519 signature: root signs device pubkey |
| `last_used_at` | TIMESTAMPTZ | Nullable, updated on use |
| `revoked_at` | TIMESTAMPTZ | Nullable, soft-delete |
| `created_at` | TIMESTAMPTZ | Immutable |

**Key invariants:**
- Maximum **10 active** (non-revoked) device keys per account, enforced at insert time with a `FOR UPDATE` lock on the account row.
- `device_kid` is globally unique — no key reuse across accounts.
- Certificates are **not rotatable**. To change a device key: revoke the old one, delegate a new one.
- Certificate message format depends on context:
  - **Signup:** root signs raw 32-byte device pubkey (no timestamp).
  - **Login:** root signs `device_pubkey (32 bytes) || timestamp_le_i64 (8 bytes)` = 40 bytes. Timestamp must be within ±300 seconds of server time.

**Device management endpoints:** Devices can be listed, added, revoked, and renamed via authenticated REST endpoints (`/auth/devices`). See [Device Management](#device-management) for details.

**Future:** Device revocation and delegation will eventually be recorded as signed envelopes in a sigchain (see [signed-envelope-spec.md](interfaces/signed-envelope-spec.md)). The spec is written but no code exists — don't add sigchain tables or dispatch logic until that feature is actively being built.

### Backup Envelope

A password-encrypted root private key stored on the server. The server holds ciphertext; decryption happens client-side only.

| Field | Type | Constraint |
|-------|------|------------|
| `id` | UUID | PK, generated |
| `account_id` | UUID | FK → `accounts`, **one per account** |
| `kid` | TEXT | Unique, denormalized root KID for join-free recovery lookup |
| `encrypted_backup` | BYTEA | Binary envelope, 90–4096 bytes |
| `salt` | BYTEA | 16 bytes, extracted from envelope |
| `version` | INTEGER | Currently always `1` |
| `created_at` | TIMESTAMPTZ | Immutable |

#### Binary format

```
Offset  Size  Field
─────────────────────────────
0       1     version       (0x01)
1       1     kdf_id        (0x01 = Argon2id)
2       4     m_cost        LE u32, ≥ 65536 (64 MiB)
6       4     t_cost        LE u32, ≥ 3
10      4     p_cost        LE u32, ≥ 1
14      16    salt
30      12    nonce         (AES-256-GCM)
42      N     ciphertext    min 48 bytes (32-byte key + 16-byte GCM tag)
```

| Constant | Value | Source |
|----------|-------|--------|
| `HEADER_SIZE` | 42 | Fixed header before ciphertext |
| `MIN_CIPHERTEXT` | 48 | 32-byte private key + 16-byte auth tag |
| `MIN_ENVELOPE_SIZE` | 90 | 42 + 48 |
| `MAX_ENVELOPE_SIZE` | 4096 | Upper bound to reject garbage |
| `MIN_M_COST` | 65536 | OWASP 2024 Argon2id minimum |
| `MIN_T_COST` | 3 | OWASP 2024 Argon2id minimum |
| `MIN_P_COST` | 1 | OWASP 2024 Argon2id minimum |

The envelope is validated at parse time (`BackupEnvelope::parse`). Weak KDF parameters, unsupported versions, or out-of-bounds sizes are rejected before the data reaches the database. The server validates the envelope structure even though it never decrypts — this catches corrupted or malicious uploads early.

**Not yet built:** Account recovery (helpers approve recovery via signed envelopes). Concept described in [signed-envelope-spec.md](interfaces/signed-envelope-spec.md) but no code exists.

### Key Identifier (KID)

A stable, short identifier derived from a public key.

**Computation:** `base64url_no_pad(SHA-256(pubkey)[0:16])`

- Input: 32-byte Ed25519 public key
- Hash: SHA-256, truncated to first 16 bytes
- Encoding: Base64url (RFC 4648 §5), no padding
- Output: **always exactly 22 characters**, alphabet `[A-Za-z0-9_-]`

The `Kid` type is a newtype in `tc-crypto` — it can only be constructed via `Kid::derive()` (from a public key) or `Kid::from_str()` / `s.parse::<Kid>()` (from a validated string). Bare strings cannot become KIDs without validation.

**Test vector:** `[1u8; 32]` → `"cs1uhCLEB_ttCYaQ8RMLfQ"`

## Trust Boundary

```
┌─────────────────────────┐      ┌─────────────────────────┐
│        Browser          │      │        Server           │
│                         │      │                         │
│  Key generation         │      │  Signature verification │
│  Certificate signing    │ ───► │  Envelope validation    │
│  Envelope encryption    │      │  Ciphertext storage     │
│  Envelope decryption    │      │  Username/KID uniqueness│
│                         │      │                         │
│  tc-crypto (WASM)       │      │  tc-crypto (native)     │
│  @noble/curves          │      │  ed25519-dalek          │
└─────────────────────────┘      └─────────────────────────┘
```

**The same `tc-crypto` crate** compiles to both native Rust (backend) and WASM (frontend). This guarantees KID derivation and base64url encoding produce identical results on both sides. See [ADR-006](decisions/006-wasm-crypto-sharing.md).

Rules:
- The server **never** handles plaintext private keys. Code that changes this is a security bug.
- The server **validates** cryptographic artifacts (signatures, envelope structure, KDF params) but does not **produce** them.
- Key generation and signing happen exclusively in the browser.
- The only encryption/decryption in the system (backup envelopes) happens client-side.

The server is still responsible for **platform-level availability and compliance**: rate limiting, abuse detection, and GDPR deletion mechanics. These are operational concerns orthogonal to the cryptographic trust model — the server enforces them because it must, not because it's trusted with key material.

## Signup Flow

All three inserts happen in a single database transaction — any failure rolls back everything.

```
Browser                              Server
───────                              ──────
Generate root key pair
Generate device key pair
Root signs device pubkey → cert
Password → Argon2id → AES-GCM
  encrypt root privkey → envelope
                                     POST /auth/signup
Pack request ──────────────────────►
                                     1. Validate username
                                     2. Decode & check root pubkey (32 bytes)
                                     3. Derive root KID
                                     4. Parse backup envelope (structure + KDF params)
                                     5. Decode & check device pubkey (32 bytes)
                                     6. Derive device KID
                                     7. Validate device name (1–128 chars)
                                     8. Verify certificate (Ed25519: root signs device)
                                     9. BEGIN TRANSACTION
                                        INSERT account
                                        INSERT backup
                                        INSERT device_key (with count check + row lock)
                                     10. COMMIT
                          ◄──────────
201 { account_id, root_kid, device_kid }
```

**Error responses:**
- **400** — validation failures: bad username, wrong key length, malformed envelope, bad device name, invalid certificate
- **409** — conflicts: username taken, key already registered (root key or device key already in use)
- **422** — max device limit reached
- **500** — database errors (details logged server-side, not exposed to client)

## Login Flow

Login recovers the root private key from the server-stored backup envelope and registers a new device key.

```
Browser                              Server
───────                              ──────
                                     GET /auth/backup/{username}
Request backup ───────────────────►
                                     1. Look up account by username
                                     2. If found with backup → return real envelope
                                        If not found → return synthetic envelope *
                          ◄──────────
200 { encrypted_backup, root_kid }

Decrypt envelope (Argon2id → AES-GCM)
  → recover root private key
Verify recovered key KID == root_kid
Generate new device key pair
Root signs (device_pubkey || timestamp_LE_i64) → cert
                                     POST /auth/login
Pack request ──────────────────────►
                                     1. Validate username, pubkey, name
                                     2. Verify timestamp within ±300s
                                     3. Look up account (401 if not found)
                                     4. Verify certificate (root signs device||ts)
                                     5. Record SHA-256(cert) as nonce (replay protection)
                                     6. INSERT device_key (with count check + row lock)
                                     7. Return device info
                          ◄──────────
201 { account_id, root_kid, device_kid }
```

\* **Anti-enumeration:** Unknown usernames receive a deterministic synthetic backup (HMAC-derived from `TC_SYNTHETIC_BACKUP_KEY`) that is structurally valid but always fails decryption. The client cannot distinguish "wrong password" from "account doesn't exist." This prevents username enumeration via the backup endpoint.

**Login error responses:**
- **400** — timestamp out of range, invalid inputs, replay detected
- **401** — unknown username OR invalid certificate (same message `"Invalid credentials"` — anti-enumeration)
- **409** — device key already registered
- **422** — max device limit reached
- **500** — database errors

## Device Management

Authenticated endpoints for managing device keys. All require [request signing](#authenticated-request-signing).

| Method | Path | Response | Description |
|--------|------|----------|-------------|
| GET | `/auth/devices` | 200 + device list | List all devices (including revoked) |
| POST | `/auth/devices` | 201 + `{ device_kid, created_at }` | Add device (cert = root signs raw pubkey) |
| DELETE | `/auth/devices/{kid}` | 204 | Revoke device (soft-delete) |
| PATCH | `/auth/devices/{kid}` | 204 | Rename device |

**Constraints:**
- Cannot self-revoke (the device making the request) — returns 422.
- Already-revoked device returns 409 on revoke or rename.
- Device not found (or belongs to different account) returns 404 — prevents device enumeration.

## Authenticated Request Signing

Authenticated REST endpoints use Ed25519 request signing (not bearer tokens). Each request includes four headers:

| Header | Format | Constraint |
|--------|--------|------------|
| `X-Device-Kid` | 22-char base64url | Must match an active device key |
| `X-Signature` | base64url Ed25519 signature (64 bytes decoded) | Signs canonical message below |
| `X-Timestamp` | Unix seconds (decimal string) | Must be within ±300s of server time |
| `X-Nonce` | Unique string | Max 64 chars, no ASCII control characters |

**Canonical message format:**

```
{METHOD}\n{PATH_AND_QUERY}\n{TIMESTAMP}\n{NONCE}\n{BODY_SHA256_HEX}
```

Example: `GET\n/auth/devices\n1700000000\ntest-nonce-abc\ne3b0c44298fc1c14...`

**Processing order** (security-critical — see `service/src/identity/http/auth.rs`):
1. Parse and validate all headers
2. Read body, compute SHA-256 hex hash, build canonical message
3. Look up device key by KID
4. **Verify signature before checking revocation** — prevents status oracle
5. Record nonce after signature verification — prevents unauthenticated nonce exhaustion
6. Check `revoked_at` — returns 403 if revoked

## Endorsement

An endorsement is a claim by a verifier that a subject account has a particular qualification. Endorsements gate voting eligibility.

| Field | Type | Constraint |
|-------|------|------------|
| `id` | UUID | PK, generated |
| `subject_id` | UUID | FK → `accounts`, the account being endorsed |
| `topic` | TEXT | e.g., `"identity_verified"`, `"authorized_verifier"` |
| `issuer_id` | UUID (nullable) | FK → `accounts`; NULL = platform genesis |
| `evidence` | JSONB (nullable) | Optional structured evidence |
| `created_at` | TIMESTAMPTZ | Immutable |
| `revoked_at` | TIMESTAMPTZ (nullable) | Soft-delete |

**Key invariants:**
- Unique constraint on `(subject_id, topic, issuer_id)` — no duplicate endorsements.
- The `"authorized_verifier"` topic is special: accounts with this endorsement can create endorsements for other accounts via `POST /verifiers/endorsements`.
- Platform verifiers are bootstrapped at startup from `TC_VERIFIERS` config (see [ADR-008](decisions/008-account-based-verifiers.md)).

**Endpoints:**

| Method | Path | Auth | Description |
|--------|------|------|-------------|
| GET | `/me/endorsements` | Yes | List caller's endorsements |
| GET | `/endorsements/check?subject_id=&topic=` | No | Check if subject has endorsement |
| POST | `/verifiers/endorsements` | Yes (verifier) | Create endorsement |
| GET | `/auth/idme/authorize` | Yes | Get ID.me OAuth redirect URL |
| GET | `/auth/idme/callback` | No (browser redirect) | ID.me OAuth callback |

**ID.me verification flow:** Users verify their identity via ID.me OAuth. The callback creates an `"identity_verified"` endorsement, enabling the user to vote. Sybil protection: each ID.me `sub` can only be linked to one TinyCongress account.

## Room

A room is a space for community discussion, containing polls on related topics.

| Field | Type | Constraint |
|-------|------|------------|
| `id` | UUID | PK, generated |
| `name` | TEXT | Required |
| `description` | TEXT (nullable) | Optional |
| `eligibility_topic` | TEXT | Default `"identity_verified"` |
| `status` | TEXT | `"open"` or `"closed"` |
| `created_at` | TIMESTAMPTZ | Immutable |

**Voting eligibility:** A room's `eligibility_topic` determines who can vote in its polls. Users must have an active endorsement with that topic. Default is `"identity_verified"` — meaning users must complete ID.me verification before voting.

### Poll

A multi-dimensional question within a room. Users rate each dimension on a configurable scale.

| Field | Type | Constraint |
|-------|------|------------|
| `id` | UUID | PK, generated |
| `room_id` | UUID | FK → `rooms` |
| `question` | TEXT | Required |
| `description` | TEXT (nullable) | Optional |
| `status` | TEXT | `"draft"`, `"active"`, or `"closed"` |
| `created_at` | TIMESTAMPTZ | Immutable |

### Dimension

A single axis of a poll (e.g., importance, urgency, feasibility).

| Field | Type | Constraint |
|-------|------|------------|
| `id` | UUID | PK, generated |
| `poll_id` | UUID | FK → `polls` |
| `name` | TEXT | Required |
| `description` | TEXT (nullable) | Optional |
| `min_value` | FLOAT | Default 0.0 |
| `max_value` | FLOAT | Default 1.0 |
| `sort_order` | INTEGER | Display ordering |

### Vote

A user's rating on a single dimension. Votes are upserted — voting again on the same dimension overwrites the previous value.

| Field | Type | Constraint |
|-------|------|------------|
| `dimension_id` | UUID | FK → `dimensions` |
| `account_id` | UUID | FK → `accounts` |
| `value` | FLOAT | Must be within `[min_value, max_value]` |
| `updated_at` | TIMESTAMPTZ | Updated on each vote |

**Rooms/Polls endpoints:**

| Method | Path | Auth | Description |
|--------|------|------|-------------|
| GET | `/rooms` | No | List open rooms |
| POST | `/rooms` | Yes | Create a room |
| GET | `/rooms/{room_id}` | No | Get room details |
| GET | `/rooms/{room_id}/polls` | No | List polls in room |
| POST | `/rooms/{room_id}/polls` | Yes | Create a poll |
| GET | `/rooms/{room_id}/polls/{poll_id}` | No | Get poll with dimensions |
| POST | `/rooms/{room_id}/polls/{poll_id}/status` | Yes | Set poll status (active/closed) |
| POST | `/rooms/{room_id}/polls/{poll_id}/dimensions` | Yes | Add dimension |
| POST | `/rooms/{room_id}/polls/{poll_id}/vote` | Yes | Cast votes (eligibility-gated) |
| GET | `/rooms/{room_id}/polls/{poll_id}/results` | No | Get aggregate results (count, mean, median, stddev per dimension) |
| GET | `/rooms/{room_id}/polls/{poll_id}/my-votes` | Yes | Get caller's votes |

**Results** include per-dimension statistics: count, mean, median, standard deviation, min, and max.

**Not yet built:** Ranking, pairing, and thread-based discussion within rooms. These will become new entity sections when work begins.

## Cross-References

| Topic | Document |
|-------|----------|
| Signed envelope JSON format | [interfaces/signed-envelope-spec.md](interfaces/signed-envelope-spec.md) |
| Shared crypto via WASM | [decisions/006-wasm-crypto-sharing.md](decisions/006-wasm-crypto-sharing.md) |
| Verifier account architecture | [decisions/008-account-based-verifiers.md](decisions/008-account-based-verifiers.md) |
| Simulation worker | [decisions/009-sim-as-api-client.md](decisions/009-sim-as-api-client.md) |
| Rust coding patterns (`Kid`, newtypes) | [interfaces/rust-coding-standards.md](interfaces/rust-coding-standards.md) |
| Error handling taxonomy | [interfaces/error-handling-backend.md](interfaces/error-handling-backend.md) |
| Secure defaults policy | [interfaces/secure-defaults.md](interfaces/secure-defaults.md) |
| Environment variables | [interfaces/environment-variables.md](interfaces/environment-variables.md) |
