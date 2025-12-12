/**
 * Key identifier (kid) derivation
 * Matches backend implementation: SHA256(pubkey) encoded as base64url
 */

import { sha256 } from '@noble/hashes/sha2.js';
import { encodeBase64Url } from './utils';

/**
 * Derive a key identifier (kid) from a public key.
 * Uses SHA-256 hash of the public key, encoded as base64url.
 *
 * @param publicKey - Ed25519 public key (32 bytes)
 * @returns Base64url-encoded SHA-256 hash
 */
export function deriveKid(publicKey: Uint8Array): string {
  const hash = sha256(publicKey);
  return encodeBase64Url(hash);
}
