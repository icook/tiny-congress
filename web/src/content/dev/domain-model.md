TinyCongress is a community governance platform built around cryptographic identity. Users generate Ed25519 key pairs client-side; the server never sees private key material. The trust model is: **the server is a dumb witness, not a trusted authority.**

## Entities

### Account

An account is the root identity in the system. It's anchored to a username and a root Ed25519 public key.

| Field | Type | Constraint |
|-------|------|------------|
| `id` | UUID | PK, generated |
| `username` | TEXT | Unique, 3-64 chars, `[a-zA-Z0-9_-]` |
| `root_pubkey` | TEXT | Base64url-encoded 32-byte Ed25519 public key |
| `root_kid` | TEXT | Unique, derived KID (see Key Identifier below) |
| `created_at` | TIMESTAMPTZ | Immutable |

The root key is the highest-privilege credential. It's meant for cold storage — used only to delegate device keys. Day-to-day operations use device keys instead.

**Username rules:**
- 3-64 characters, ASCII `[a-zA-Z0-9_-]` only
- Trimmed before validation
- Case-insensitive match against a reserved list (`admin`, `root`, `system`, etc.)

### Device Key

A delegated Ed25519 key for daily use. The root key signs a certificate over the device key to prove authorization.

| Field | Type | Constraint |
|-------|------|------------|
| `id` | UUID | PK, generated |
| `account_id` | UUID | FK to accounts, indexed |
| `device_kid` | TEXT | Globally unique KID |
| `device_pubkey` | TEXT | Base64url 32-byte Ed25519 public key |
| `device_name` | TEXT | 1-128 chars, user-provided |
| `certificate` | BYTEA | 64-byte Ed25519 signature: root signs device pubkey |
| `last_used_at` | TIMESTAMPTZ | Nullable, updated on use |
| `revoked_at` | TIMESTAMPTZ | Nullable, soft-delete |
| `created_at` | TIMESTAMPTZ | Immutable |

**Key invariants:**
- Maximum **10 active** (non-revoked) device keys per account, enforced at insert time with a row lock.
- `device_kid` is globally unique — no key reuse across accounts.
- Certificates are **not rotatable**. To change a device key: revoke the old one, delegate a new one.
- Certificate message format depends on context:
  - **Signup:** root signs raw 32-byte device pubkey (no timestamp).
  - **Login:** root signs `device_pubkey (32 bytes) || timestamp_le_i64 (8 bytes)` = 40 bytes. Timestamp must be within +/-300 seconds of server time.

### Backup Envelope

A password-encrypted root private key stored on the server. The server holds ciphertext; decryption happens client-side only.

| Field | Type | Constraint |
|-------|------|------------|
| `id` | UUID | PK, generated |
| `account_id` | UUID | FK to accounts, **one per account** |
| `kid` | TEXT | Unique, denormalized root KID for join-free recovery lookup |
| `encrypted_backup` | BYTEA | Binary envelope, 90-4096 bytes |
| `salt` | BYTEA | 16 bytes, extracted from envelope |
| `version` | INTEGER | Currently always `1` |
| `created_at` | TIMESTAMPTZ | Immutable |

#### Binary format

```
Offset  Size  Field
0       1     version       (0x01)
1       1     kdf_id        (0x01 = Argon2id)
2       4     m_cost        LE u32, >= 65536 (64 MiB)
6       4     t_cost        LE u32, >= 3
10      4     p_cost        LE u32, >= 1
14      16    salt
30      12    nonce         (AES-256-GCM)
42      N     ciphertext    min 48 bytes (32-byte key + 16-byte GCM tag)
```

The envelope is validated at parse time. Weak KDF parameters, unsupported versions, or out-of-bounds sizes are rejected before the data reaches the database.

#### Anti-enumeration: synthetic backups

`GET /auth/backup/{username}` returns `200 OK` for every username — real or fake. For unknown users, the server generates a **synthetic backup** that is indistinguishable from a real one until decryption fails. This prevents username enumeration via the backup endpoint.

### Key Identifier (KID)

A stable, short identifier derived from a public key.

**Computation:** `base64url_no_pad(SHA-256(pubkey)[0:16])`

- Input: 32-byte Ed25519 public key
- Hash: SHA-256, truncated to first 16 bytes
- Encoding: Base64url (RFC 4648), no padding
- Output: **always exactly 22 characters**, alphabet `[A-Za-z0-9_-]`

## Trust Boundary

```
Browser                          Server
  Key generation                   Signature verification
  Certificate signing              Envelope validation
  Envelope encryption              Ciphertext storage
  Envelope decryption              Username/KID uniqueness

  tc-crypto (WASM)                 tc-crypto (native)
  @noble/curves                    ed25519-dalek
```

**The same `tc-crypto` crate** compiles to both native Rust (backend) and WASM (frontend). This guarantees KID derivation and base64url encoding produce identical results on both sides.

Rules:
- The server **never** handles plaintext private keys. Code that changes this is a security bug.
- The server **validates** cryptographic artifacts (signatures, envelope structure, KDF params) but does not **produce** them.
- Key generation and signing happen exclusively in the browser.
- The only encryption/decryption in the system (backup envelopes) happens client-side.

## Signup Flow

All three inserts happen in a single database transaction — any failure rolls back everything.

```
Browser                              Server
Generate root key pair
Generate device key pair
Root signs device pubkey -> cert
Password -> Argon2id -> AES-GCM
  encrypt root privkey -> envelope
                                     POST /auth/signup
                                     1. Validate username
                                     2. Decode & check root pubkey (32 bytes)
                                     3. Derive root KID
                                     4. Parse backup envelope (structure + KDF params)
                                     5. Decode & check device pubkey (32 bytes)
                                     6. Derive device KID
                                     7. Validate device name (1-128 chars)
                                     8. Verify certificate (Ed25519: root signs device)
                                     9. BEGIN TRANSACTION
                                        INSERT account
                                        INSERT backup
                                        INSERT device_key
                                     10. COMMIT
201 { account_id, root_kid, device_kid }
```

## Login Flow

Login recovers the root private key from the server-stored backup envelope and registers a new device key.

```
Browser                              Server
                                     GET /auth/backup/{username}
                                     1. If found with backup -> return real envelope
                                        If not found -> return synthetic envelope
200 { encrypted_backup, root_kid }

Decrypt envelope (Argon2id -> AES-GCM)
  -> recover root private key
Verify recovered key KID == root_kid
Generate new device key pair
Root signs (device_pubkey || timestamp) -> cert
                                     POST /auth/login
                                     1. Validate username, pubkey, name
                                     2. Verify timestamp within +/-300s
                                     3. Look up account (401 if not found)
                                     4. Verify certificate
                                     5. Record nonce (replay protection)
                                     6. INSERT device_key
201 { account_id, root_kid, device_kid }
```

## Authenticated Request Signing

Authenticated REST endpoints use Ed25519 request signing (not bearer tokens). Each request includes four headers:

| Header | Format | Constraint |
|--------|--------|------------|
| `X-Device-Kid` | 22-char base64url | Must match an active device key |
| `X-Signature` | base64url Ed25519 signature | Signs canonical message below |
| `X-Timestamp` | Unix seconds (decimal string) | Must be within +/-300s of server time |
| `X-Nonce` | Unique string | Max 64 chars, no ASCII control characters |

**Canonical message format:**

```
{METHOD}\n{PATH_AND_QUERY}\n{TIMESTAMP}\n{NONCE}\n{BODY_SHA256_HEX}
```

## Endorsement

An endorsement is a claim by a verifier that a subject account has a particular qualification. Endorsements gate voting eligibility.

| Field | Type | Constraint |
|-------|------|------------|
| `id` | UUID | PK, generated |
| `subject_id` | UUID | FK to accounts, the account being endorsed |
| `topic` | TEXT | e.g., `"identity_verified"` |
| `issuer_id` | UUID (nullable) | FK to accounts; NULL = platform genesis |
| `evidence` | JSONB (nullable) | Optional structured evidence |
| `created_at` | TIMESTAMPTZ | Immutable |
| `revoked_at` | TIMESTAMPTZ (nullable) | Soft-delete |

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

**Voting eligibility:** A room's `eligibility_topic` determines who can vote in its polls. Users must have an active endorsement with that topic.

### Poll

A multi-dimensional question within a room. Users rate each dimension on a configurable scale.

### Dimension

A single axis of a poll (e.g., importance, urgency, feasibility). Each has a configurable min/max range and display labels.

### Vote

A user's rating on a single dimension. Votes are upserted — voting again on the same dimension overwrites the previous value. Values must be within the dimension's configured range.
