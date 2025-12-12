# Signed Envelope Specification

Cryptographic envelope format for authenticated actions across the system.

## Overview

All state-changing operations (account creation, device delegation, endorsements, recovery) use signed envelopes to ensure authenticity and create an auditable sigchain.

## Envelope Structure

```json
{
  "v": 1,
  "payload_type": "DeviceDelegation",
  "payload": {
    "device_id": "550e8400-e29b-41d4-a716-446655440000",
    "prev_hash": "base64url-encoded-hash-or-null"
  },
  "signer": {
    "account_id": "550e8400-e29b-41d4-a716-446655440001",
    "device_id": "550e8400-e29b-41d4-a716-446655440002",
    "kid": "base64url-encoded-sha256-of-pubkey"
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
| `device_id` | `uuid \| null` | Device performing the action |
| `kid` | `string` | Key identifier (SHA-256 of public key, base64url) |

## Cryptographic Details

### Key Identifier (kid)

```
kid = base64url_no_pad(SHA-256(public_key))
```

- Input: 32-byte Ed25519 public key
- Output: 43-character base64url string (no padding)

### Signature Algorithm

**Ed25519** (RFC 8032) over canonical signing bytes.

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

| Type | Purpose | Required Signer |
|------|---------|-----------------|
| `DeviceDelegation` | Add device to account | Root key |
| `DeviceRevocation` | Revoke a device | Root key |
| `Endorsement` | Create endorsement | Device key |
| `EndorsementRevocation` | Revoke endorsement | Device key |
| `RecoveryPolicySet` | Set recovery policy | Root key |
| `RecoveryApproval` | Approve recovery request | Helper's device key |
| `RootRotation` | Rotate root key | New root key |

## Verification Steps

1. **Decode signature**: Base64url decode `sig`
2. **Derive expected kid**: `SHA-256(signer_pubkey)` → base64url
3. **Verify kid matches**: `envelope.signer.kid == expected_kid`
4. **Canonicalize**: Build signing bytes from `payload_type`, `payload`, `signer`
5. **Verify signature**: Ed25519 verify over canonical bytes

```rust
// Backend (Rust)
pub fn verify_envelope(envelope: &SignedEnvelope, pubkey: &[u8]) -> Result<(), CryptoError> {
    let canonical_bytes = envelope.canonical_signing_bytes()?;
    let sig_bytes = envelope.signature_bytes()?;
    verify_signature(&canonical_bytes, pubkey, &sig_bytes)?;

    let expected_kid = derive_kid(pubkey);
    if envelope.signer.kid != expected_kid {
        return Err(CryptoError::KidMismatch);
    }
    Ok(())
}
```

```typescript
// Frontend (TypeScript)
export function verifyEnvelope(envelope: SignedEnvelope, publicKey: Uint8Array): void {
  const expectedKid = deriveKid(publicKey);
  if (envelope.signer.kid !== expectedKid) {
    throw new CryptoError('Kid mismatch');
  }

  const signingBytes = canonicalSigningBytes(envelope);
  const signature = decodeBase64Url(envelope.sig);

  if (!ed25519.verify(signature, signingBytes, publicKey)) {
    throw new CryptoError('Signature verification failed');
  }
}
```

## Encoding

All binary data uses **base64url without padding** (RFC 4648 §5):
- Alphabet: `A-Za-z0-9-_`
- No `=` padding characters

## Sigchain Integration

Envelopes are stored in `sigchain_events` table with sequence numbers per account. The `prev_hash` field in payloads enables hash-chaining for tamper detection.

## Implementation Locations

| Component | Location |
|-----------|----------|
| Backend types | `service/src/identity/crypto/envelope.rs` |
| Backend verification | `service/src/identity/crypto/mod.rs` |
| Frontend signing | `web/src/features/identity/keys/signer.ts` |
| Frontend kid derivation | `web/src/features/identity/keys/kid.ts` |
| Frontend canonicalization | `web/src/features/identity/keys/canonical.ts` |
