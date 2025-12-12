/**
 * Envelope signing and verification
 * Matches backend SignedEnvelope implementation
 */

import { ed25519 } from '@noble/curves/ed25519.js';
import { canonicalizeValue } from './canonical';
import { deriveKid } from './kid';
import { CryptoError, type EnvelopeSigner, type SignedEnvelope } from './types';
import { decodeBase64Url, encodeBase64Url } from './utils';

/**
 * Create the canonical signing bytes for an envelope.
 * Matches backend: canonicalize({ payload_type, payload, signer })
 *
 * @param envelope - Envelope to sign (sig field ignored)
 * @returns Canonical bytes to sign
 */
function canonicalSigningBytes(envelope: Omit<SignedEnvelope, 'sig'>): Uint8Array {
  const canonicalTarget = {
    payload_type: envelope.payload_type,
    payload: envelope.payload,
    signer: envelope.signer,
  };
  return canonicalizeValue(canonicalTarget);
}

/**
 * Sign an envelope with a private key.
 *
 * @param payloadType - Type identifier for the payload
 * @param payload - Envelope payload (will be canonicalized)
 * @param signer - Signer metadata
 * @param privateKey - Ed25519 private key (32 bytes)
 * @returns Complete signed envelope
 */
export function signEnvelope(
  payloadType: string,
  payload: unknown,
  signer: EnvelopeSigner,
  privateKey: Uint8Array
): SignedEnvelope {
  const envelope: Omit<SignedEnvelope, 'sig'> = {
    v: 1,
    payload_type: payloadType,
    payload,
    signer,
  };

  const signingBytes = canonicalSigningBytes(envelope);
  const signature = ed25519.sign(signingBytes, privateKey);
  const sig = encodeBase64Url(signature);

  return {
    ...envelope,
    sig,
  };
}

/**
 * Verify an envelope signature.
 *
 * @param envelope - Signed envelope
 * @param publicKey - Ed25519 public key (32 bytes)
 * @throws {CryptoError} If signature is invalid or kid doesn't match
 */
export function verifyEnvelope(envelope: SignedEnvelope, publicKey: Uint8Array): void {
  // Verify kid matches public key
  const expectedKid = deriveKid(publicKey);
  if (envelope.signer.kid !== expectedKid) {
    throw new CryptoError('Kid mismatch');
  }

  // Verify signature
  const signingBytes = canonicalSigningBytes(envelope);
  const signature = decodeBase64Url(envelope.sig);

  const isValid = ed25519.verify(signature, signingBytes, publicKey);
  if (!isValid) {
    throw new CryptoError('Signature verification failed');
  }
}

/**
 * Sign a challenge response for authentication.
 * Used by FE-03 login flow.
 *
 * @param challengeId - UUID of the challenge
 * @param nonce - Challenge nonce from server
 * @param accountId - Account UUID
 * @param deviceId - Device UUID
 * @param privateKey - Device private key
 * @returns Base64url-encoded signature
 */
export function signChallenge(
  challengeId: string,
  nonce: string,
  accountId: string,
  deviceId: string,
  privateKey: Uint8Array
): string {
  const payload = {
    challenge_id: challengeId,
    nonce,
    account_id: accountId,
    device_id: deviceId,
  };

  const canonical = canonicalizeValue(payload);
  const signature = ed25519.sign(canonical, privateKey);
  return encodeBase64Url(signature);
}
