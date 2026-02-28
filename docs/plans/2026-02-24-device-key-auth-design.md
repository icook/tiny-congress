# Device Key Authentication System Design

**Date:** 2026-02-24
**Status:** Draft
**Related:** [ADR-006 (PR #182)](https://github.com/icook/tiny-congress/pull/182) | [ADR-007 (PR #182)](https://github.com/icook/tiny-congress/pull/182)

## Overview

Password-locked root key that issues delegated device keys for daily signing. Inspired by Bitwarden's architecture but with an SSH-CA-like delegation model: the root key acts as a certificate authority, each device gets its own Ed25519 keypair blessed by the root key, and users manage their devices through a settings UI.

## Key Hierarchy

```
Master Password
    │
    ├─ (Argon2id) ──► Encryption Key ──► encrypts Root Private Key
    │                                         │
    │                                         ▼
    │                                    Server Backup
    │                                    (encrypted blob)
    │
Root Key (Ed25519)
    │
    ├─ signs ──► Device Certificate A  ──► Device Key A (this laptop)
    ├─ signs ──► Device Certificate B  ──► Device Key B (phone)
    └─ signs ──► Device Certificate C  ──► Device Key C (work PC)
```

**Root key** is the identity anchor. It only gets unlocked (decrypted from server backup) when authorizing a new device or recovering. It never persists in memory during normal use.

**Device keys** are the workhorses. Each device has its own Ed25519 keypair. The root key signs a "certificate" blessing each device key. Day-to-day operations use the device key with no password required.

**Server backup** holds the root private key encrypted under the user's password. The server cannot read it. If all devices are lost, the user recovers by entering username + password on a new device.

## Design Decisions

### Device keys are delegated signing keys

Each device gets its own Ed25519 keypair, authorized by the root key's signature over a device certificate. This differs from PR #182's design where the root key is the daily signer.

**Why this is better:**
- Root key exposure is rare and transient (unlock → sign cert → zeroize)
- Device compromise means revoking one device, not rotating the whole identity
- No complex CryptoKey persistence/TTL/Worker machinery for daily signing
- The device key is a fresh WebCrypto key — non-extractable from the start, never exists as raw bytes

### Worker handles root key operations only

A dedicated Web Worker handles password KDF, root key decryption, device certificate signing, and zeroization. Device keys use WebCrypto directly on the main thread.

**Rationale:** Device keys are generated as `extractable: false` WebCrypto keys — raw bytes never exist in JS, so there's nothing for a Worker to protect. The root key's raw bytes do briefly exist during device authorization, so the Worker keeps them off the main thread where XSS could reach them. Since root key operations are rare (new device, recovery) and the user is already waiting on Argon2id, Worker overhead is invisible.

### No WASM fallback for device signing

Ed25519 WebCrypto coverage is ~95%+ (Feb 2026: Chrome 113+, Safari 17+, Firefox 128+). For unsupported browsers, we show a warning rather than maintaining a parallel WASM signing path. This halves the test surface. Verification still uses WASM (ZIP215 semantics per ADR-007).

### Any authorized device can revoke any other device

A device signs a revocation request with its own device key. The server verifies the requesting device belongs to the same account and isn't itself revoked. This avoids requiring the password to revoke a stolen device from another device.

## Database Schema

### Existing tables (no changes)

```sql
-- accounts (from migration 03)
-- account_backups (from PR #182 / migration 04, to be landed separately)
```

### New table: device_keys

```sql
CREATE TABLE device_keys (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    account_id UUID NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    device_kid TEXT NOT NULL UNIQUE,
    device_pubkey TEXT NOT NULL,          -- base64url Ed25519 public key
    device_name TEXT NOT NULL,            -- e.g. "Chrome on macOS"
    certificate BYTEA NOT NULL,           -- root key's Ed25519 signature over canonical cert message
    last_used_at TIMESTAMPTZ,
    revoked_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_device_keys_account ON device_keys(account_id);
CREATE INDEX idx_device_keys_kid ON device_keys(device_kid);
```

### Device certificate format

The certificate is the root key's Ed25519 signature over a canonical message built by the WASM module:

```
tc:device-cert:v1:<root_kid>:<device_kid>:<device_pubkey_b64url>:<device_name>:<created_at_unix>
```

Anyone with the root public key can verify a device certificate without hitting the server.

## Encrypted Backup Format

Carried forward from PR #182's design:

```
+--------+--------+----------+------+-------+-----------+
| Version| KDF ID | KDF Params| Salt | Nonce | Ciphertext|
| 1 byte | 1 byte | variable | 16 B | 12 B  | 48 bytes  |
+--------+--------+----------+------+-------+-----------+
```

| Field | Size | Description |
|-------|------|-------------|
| Version | 1 byte | `0x01` |
| KDF ID | 1 byte | `0x01` = Argon2id, `0x02` = PBKDF2 |
| KDF Params | variable | Argon2: 12 bytes (m:4, t:4, p:4). PBKDF2: 4 bytes (iterations) |
| Salt | 16 bytes | Random |
| Nonce | 12 bytes | AES-GCM nonce |
| Ciphertext | 48 bytes | AES-256-GCM(root_private_key \|\| tag) |

**KDF parameters:**
- Argon2id: m=19456 KiB, t=2, p=1 (OWASP recommended)
- PBKDF2-SHA256: 600,000 iterations (fallback)

## API Endpoints

### Modified: POST /auth/signup

Atomic creation of account + backup + first device key.

```
POST /auth/signup
{
  "username": "alice",
  "root_pubkey": "<base64url>",
  "backup": {
    "encrypted_blob": "<base64url envelope>",
  },
  "device": {
    "pubkey": "<base64url>",
    "name": "Chrome on macOS",
    "certificate": "<base64url signature>"
  }
}

201 Created
{
  "account_id": "<uuid>",
  "root_kid": "<base64url>",
  "device_kid": "<base64url>"
}
```

### Kept from PR #182: Backup endpoints

```
GET    /auth/backup/:root_kid    → encrypted backup blob (rate limited: 5/min/IP)
POST   /auth/backup              → create/update backup (authenticated)
DELETE /auth/backup/:root_kid    → remove backup (authenticated)
```

### New: Device endpoints

All device endpoints require a signed request from a non-revoked device on the same account.

```
GET    /auth/devices             → list devices for account
POST   /auth/devices             → register new device key + certificate
DELETE /auth/devices/:device_kid → revoke a device
PATCH  /auth/devices/:device_kid → rename a device
```

## Flows

### Signup (Milestone 1)

```
User enters: username + password + confirm
                     │
    ┌── Main Thread ─┴──────────────────────────────┐
    │  1. Generate device keypair via WebCrypto       │
    │     (extractable: false, usages: ['sign'])      │
    │  2. Export device public key bytes              │
    │  3. Derive device_kid via WASM                  │
    └────────────────┬───────────────────────────────┘
                     │ password + device pubkey to Worker
    ┌── Crypto Worker ┴─────────────────────────────┐
    │  4. Generate root Ed25519 keypair (raw bytes)  │
    │  5. Derive root_kid via WASM                   │
    │  6. Build canonical cert message via WASM      │
    │  7. Sign device certificate with root key      │
    │  8. Argon2id(password, salt) → encryption key  │
    │  9. AES-256-GCM encrypt root private key       │
    │  10. Zeroize root private key + password       │
    │  11. Return: root_pubkey, root_kid, backup     │
    │      blob, device certificate                  │
    └────────────────┬───────────────────────────────┘
                     │
    POST /auth/signup (atomic: account + backup + device)
                     │
    IndexedDB: store device CryptoKey, device_kid,
               account_id, root_kid, username
```

### Authorize New Device (future milestone)

```
New device → enter username + password
                     │
    GET /auth/backup/:root_kid → encrypted blob
                     │
    ┌── Main Thread ─┴──────────────────────────────┐
    │  1. Generate device keypair via WebCrypto       │
    │  2. Export device public key bytes              │
    └────────────────┬───────────────────────────────┘
                     │ password + device pubkey to Worker
    ┌── Crypto Worker ┴─────────────────────────────┐
    │  3. Argon2id(password, salt) → encryption key  │
    │  4. Decrypt root private key                   │
    │  5. Sign device certificate with root key      │
    │  6. Zeroize root private key + password        │
    └────────────────┬───────────────────────────────┘
                     │
    POST /auth/devices { root_kid, device pubkey+cert }
                     │
    IndexedDB: store device CryptoKey + metadata
```

### Revoke Device

```
Any non-revoked device on the account
    │
    │ device key signs revocation request
    │
    DELETE /auth/devices/:device_kid
    │
    Server: verify requesting device belongs to same
            account and is not itself revoked.
            Set revoked_at on target device.
```

### Change Password / Re-backup

```
Enter old password + new password
    │
    ┌── Crypto Worker ──────────────────────────────┐
    │  1. Argon2id(old password, old salt) → decrypt │
    │  2. Argon2id(new password, new salt) → encrypt │
    │  3. Zeroize all key material                   │
    └────────────────┬───────────────────────────────┘
                     │
    POST /auth/backup (update encrypted blob)
```

## Frontend Architecture

### File structure

```
web/src/features/identity/
├── keys/
│   ├── crypto.ts               # existing - key generation
│   ├── types.ts                # existing - KeyPair interface
│   ├── device-cert.ts          # NEW - certificate message building
│   └── index.ts
├── worker/
│   ├── crypto.worker.ts        # NEW - root key operations
│   ├── types.ts                # NEW - worker message types
│   └── index.ts
├── storage/
│   ├── device-store.ts         # NEW - IndexedDB for device CryptoKey
│   └── index.ts
├── api/
│   ├── client.ts               # existing - extend with device endpoints
│   ├── queries.ts              # existing - extend with device queries
│   └── index.ts
├── components/
│   ├── SignupForm.tsx           # existing - add password fields
│   ├── KeyDashboard.tsx         # NEW - settings page
│   ├── DeviceList.tsx           # NEW - device table
│   ├── DeviceActions.tsx        # NEW - revoke/rename
│   ├── BackupStatus.tsx         # NEW - root key backup info
│   └── index.ts
└── index.ts
```

### Crypto Worker messages

```typescript
type WorkerRequest =
  | { type: 'signup'; password: string; devicePubkey: Uint8Array }
  | { type: 'authorize-device'; password: string; encryptedBackup: Uint8Array;
      devicePubkey: Uint8Array; deviceName: string }
  | { type: 'change-password'; oldPassword: string; newPassword: string;
      encryptedBackup: Uint8Array };

type WorkerResponse =
  | { type: 'signup-result'; rootPubkey: Uint8Array; rootKid: string;
      backupBlob: Uint8Array; certificate: Uint8Array }
  | { type: 'authorize-result'; certificate: Uint8Array }
  | { type: 'rebackup-result'; backupBlob: Uint8Array }
  | { type: 'error'; message: string };
```

### IndexedDB schema

```typescript
interface StoredDeviceIdentity {
  deviceKey: CryptoKey;       // non-extractable, sign-only
  deviceKid: string;
  accountId: string;
  rootKid: string;
  username: string;
}
```

No TTL needed — device keys are valid until explicitly revoked. Logout clears the store.

## Key Management UI

### Identity indicator (header)

Always visible when logged in. Shows username, current device name, green status dot.

### Settings > Keys page

**Root key section:**
- Root KID (truncated with copy button)
- Backup status: "Last backed up: <date>" or "No backup"
- "Change Password" button → modal with old/new password fields

**Device list table:**

| Name | Device KID | Created | Last Used | Status | Actions |
|------|-----------|---------|-----------|--------|---------|
| Chrome on macOS | `abc...` (copy) | 2026-02-24 | 2 min ago | Active (this device) | Rename |
| Firefox on Linux | `def...` (copy) | 2026-02-20 | 3 days ago | Active | Rename, Revoke |
| Safari on iPhone | `ghi...` (copy) | 2026-01-15 | — | Revoked | — |

- Current device highlighted, cannot self-revoke
- Revoke shows confirmation dialog
- Rename is inline edit
- Revoked devices shown greyed out with revocation date

## Implementation Milestones

### Milestone 1: Signup with persistence

Fix the immediate gap — keys survive page reload.

**Backend:**
- `device_keys` migration
- `DeviceKeyRepo` trait + `PgDeviceKeyRepo`
- Revised `/auth/signup` endpoint (atomic: account + backup + device)
- Device certificate verification in WASM module

**Frontend:**
- Crypto Worker (signup path only)
- IndexedDB device key storage
- Revised SignupForm with password fields
- Identity indicator in header (shows logged-in state)

**Tests:**
- Signup round-trip (generate keys → API → persist → reload → still authenticated)
- Certificate verification (WASM cross-check)
- Worker message passing
- IndexedDB persistence

### Milestone 2: Device management UI

**Backend:**
- `GET/POST/DELETE/PATCH /auth/devices` endpoints
- Request authentication via device key signatures

**Frontend:**
- Settings > Keys page
- Device list with revoke/rename
- Backup status display

### Milestone 3: Multi-device authorization

**Backend:**
- Login/recovery flow (fetch backup → decrypt → authorize device)
- Rate limiting on backup retrieval

**Frontend:**
- Login page (username + password → device authorization)
- "Authorize new device" flow
- Change password / re-backup flow

### Milestone 4: Hardening

- Rate limiting on recovery endpoint
- CSP headers
- Audit logging for auth events
- Security review

## Security Model

| Threat | Mitigation | Residual Risk |
|--------|------------|---------------|
| Server compromise | Root key encrypted client-side; server never sees plaintext | Attacker gets encrypted blobs, can attempt offline brute force |
| XSS on main thread | Device key is non-extractable CryptoKey; root key operations in Worker | Attacker can call `sign()` on device key while page is open |
| Stolen device | Revoke from another device (no password needed) | Attacker can sign until revocation propagates |
| Weak password | Argon2id with high memory cost | User education; consider password strength meter |
| All devices lost | Server backup recovery with username + password | Depends on password strength |
| Malicious extension | Non-extractable prevents key export | Extension can invoke signing |

## Relationship to PR #182

PR #182 (`docs/180-webcrypto-key-recovery-adr`) contains:
- ADR-006 and ADR-007 (draft, not merged)
- Implementation spec with 6-phase plan
- Phase 1 implementation: `account_backups` migration, backup API, repository layer, tests

This design **supersedes** PR #182's implementation spec but **builds on its ideas**:
- The `account_backups` schema is adopted unchanged
- The encrypted backup envelope format is adopted unchanged
- ADR-006 and ADR-007 should be updated to reflect the device key model
- The backup API endpoints will be re-implemented as part of the new atomic signup

PR #182 should remain open for reference but not be merged as-is.

## Open Questions

1. **Device name auto-detection:** Use `navigator.userAgent` parsing, or let users name their device during signup?
2. **Max devices per account:** Cap at some reasonable limit (e.g., 10)?
3. **Revoked device cleanup:** Keep revoked device records forever, or garbage collect after N days?
4. **Password strength:** Minimum length? Strength meter? Reject weak passwords?
5. **Offline signing:** Should device keys work fully offline, or require periodic server check-in to confirm non-revocation?
