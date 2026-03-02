# Signed Envelope Specification

Cryptographic envelope format for authenticated identity mutations.

> **Implementation status:** This spec is a design target. The envelope format and sigchain
> are **not yet implemented**. The current system uses direct Ed25519 signatures for device
> certificates and nonce-based HTTP request signing for authenticated endpoints. See
> [ADR-008](../decisions/008-identity-model.md) for what is implemented today.

## Overview

Identity mutations (device delegation, revocation, endorsements, key rotation) will use signed envelopes to ensure authenticity and create an auditable sigchain. Read-only operations and session authentication use nonce-based HTTP request signing instead — see [Current Authentication](#current-authentication) for details.

## Envelope Structure

```json
{
  "v": 1,
  "payload_type": "DeviceDelegation",
  "payload": {
    "device_kid": "base64url-kid-22-chars",
    "prev_hash": "base64url-encoded-hash-or-null"
  },
  "signer": {
    "account_id": "550e8400-e29b-41d4-a716-446655440001",
    "kid": "base64url-kid-22-chars"
  },
  "sig": "base64url-encoded-ed25519-signature"
}
```

### Fields

| Field | Type | Description |
|-------|------|-------------|
| `v` | `u8` | Envelope version (currently `1`) |
| `payload_type` | `string` | Action type identifier |
| `payload` | `object` | Action-specific data |
| `signer` | `object` | Identity of the signer |
| `sig` | `string` | Base64url-encoded Ed25519 signature |

### Signer Object

| Field | Type | Description |
|-------|------|-------------|
| `account_id` | `uuid \| null` | Account performing the action |
| `kid` | `string` | Key identifier (22-character truncated hash — see below) |

## Cryptographic Details

### Key Identifier (kid)

```
kid = base64url_no_pad(SHA-256(public_key)[0:16])
```

- Input: 32-byte Ed25519 public key
- Hash: SHA-256, truncated to first 16 bytes
- Output: 22-character base64url string (no padding)
- Implemented in: `crates/tc-crypto/src/kid.rs` (`Kid::derive`)

The `Kid` newtype enforces these invariants at the type level — it cannot be constructed without validation. See [ADR-008](../decisions/008-identity-model.md) for design rationale.

### Signature Algorithm

**Ed25519** (RFC 8032) with **strict verification** (`verify_strict` via `ed25519-dalek`). Strict verification rejects malleable signatures.

### Signing Bytes

The signature covers a canonicalized JSON object containing:

```json
{
  "payload_type": "...",
  "payload": { ... },
  "signer": { ... }
}
```

**Note**: The `v` and `sig` fields are NOT included in signing bytes.

### Canonicalization

Uses **RFC 8785 (JSON Canonicalization Scheme)**:
- Sorted object keys (lexicographic)
- No whitespace
- Unicode normalization
- Deterministic number formatting

### Signature

```
sig = base64url_no_pad(ed25519_sign(canonical_bytes, private_key))
```

## Payload Types

| Type | Purpose | Required Signer | Status |
|------|---------|-----------------|--------|
| `DeviceDelegation` | Add device to account | Root key | Planned — currently uses direct signature |
| `DeviceRevocation` | Revoke a device | Root key | Planned |
| `Endorsement` | Create endorsement | Device key | Planned |
| `EndorsementRevocation` | Revoke endorsement | Device key | Planned |
| `RecoveryPolicySet` | Set recovery policy | Root key | Planned |
| `RecoveryApproval` | Approve recovery request | Helper's device key | Planned |
| `RootRotation` | Rotate root key | New root key | Planned |

## Verification Steps

1. **Decode signature**: Base64url decode `sig`
2. **Derive expected kid**: `SHA-256(signer_pubkey)[0:16]` → base64url (22 chars)
3. **Verify kid matches**: `envelope.signer.kid == expected_kid`
4. **Canonicalize**: Build signing bytes from `payload_type`, `payload`, `signer`
5. **Verify signature**: Ed25519 strict verify over canonical bytes

## Encoding

All binary data uses **base64url without padding** (RFC 4648 §5):
- Alphabet: `A-Za-z0-9-_`
- No `=` padding characters

## Sigchain Integration

Envelopes will be stored in a `sigchain_events` table with sequence numbers per account. The `prev_hash` field in payloads enables hash-chaining for tamper detection.

The sigchain provides replay protection for identity mutations — an envelope with a `prev_hash` that doesn't match the current chain head is rejected. This makes nonce-based replay prevention redundant for sigchain operations (but not for HTTP request authentication — see below).

## Current Authentication

The system currently uses two authentication mechanisms, neither of which involves the signed envelope format:

1. **Device certificates** (at signup): The root key signs the raw 32-byte device public key. This direct signature serves as the certificate. Replay is prevented by DB uniqueness constraints on `device_kid`. See [ADR-008](../decisions/008-identity-model.md).

2. **Nonce-based HTTP request signing** (for authenticated endpoints): Each request includes `X-Device-Kid`, `X-Signature`, `X-Timestamp`, and `X-Nonce` headers. The device key signs the request, and the nonce prevents replay. This mechanism is correct for read-only operations and will remain even after sigchain is introduced.

### Migration path

When the sigchain is implemented:
- Identity mutations will transition from authenticated HTTP requests to envelope submission
- HTTP request signing (nonce-based) stays for non-mutation endpoints
- Existing device certificates (raw pubkey signatures) will be grandfathered as pre-sigchain entries
- The `SignedEnvelope` Rust type and JCS canonicalization will be added to `crates/tc-crypto`

## Implementation Locations

### Implemented today

| Component | Location |
|-----------|----------|
| KID derivation | `crates/tc-crypto/src/kid.rs` |
| Base64url encoding | `crates/tc-crypto/src/lib.rs` |
| Ed25519 verification | `crates/tc-crypto/src/lib.rs` (`verify_ed25519`) |
| BackupEnvelope (encrypted key format) | `crates/tc-crypto/src/envelope.rs` |
| Identity service (validation + signup) | `service/src/identity/service.rs` |
| Identity repo (persistence) | `service/src/identity/repo/` |
| Frontend KID derivation (WASM) | `web/src/wasm/tc-crypto/` |
| Frontend key generation | `web/src/features/identity/keys/crypto.ts` |

### Planned (not yet implemented)

| Component | Proposed Location |
|-----------|-------------------|
| SignedEnvelope type | `crates/tc-crypto/src/signed_envelope.rs` |
| JCS canonicalization | `crates/tc-crypto/src/canonical.rs` |
| Sigchain events table | `service/migrations/` |
| Frontend envelope signing | `web/src/features/identity/keys/signer.ts` |
| Frontend canonicalization | `web/src/features/identity/keys/canonical.ts` |
