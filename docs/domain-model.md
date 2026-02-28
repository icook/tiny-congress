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

**Not yet built:** Authentication (login), GDPR account deletion. No code exists for these — don't scaffold prematurely.

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
- Certificate message is the raw 32-byte device public key. No additional framing or context bytes.

**Not yet built:** Device revocation and delegation will eventually be recorded as signed envelopes in a sigchain (see [signed-envelope-spec.md](interfaces/signed-envelope-spec.md)). The spec is written but no code exists — don't add sigchain tables or dispatch logic until that feature is actively being built.

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

The only implemented mutation. All three inserts happen in a single database transaction — any failure rolls back everything.

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

**Not yet built:** Voting, ranking, and pairing are referenced in directory conventions and test guidelines but have no spec, schema, or code. These will become new entity sections in this document when work begins.

## Cross-References

| Topic | Document |
|-------|----------|
| Signed envelope JSON format | [interfaces/signed-envelope-spec.md](interfaces/signed-envelope-spec.md) |
| Shared crypto via WASM | [decisions/006-wasm-crypto-sharing.md](decisions/006-wasm-crypto-sharing.md) |
| Rust coding patterns (`Kid`, newtypes) | [interfaces/rust-coding-standards.md](interfaces/rust-coding-standards.md) |
| Error handling taxonomy | [interfaces/error-handling-backend.md](interfaces/error-handling-backend.md) |
| Secure defaults policy | [interfaces/secure-defaults.md](interfaces/secure-defaults.md) |
