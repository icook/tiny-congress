/**
 * Cryptographic operations for Ed25519 keys
 * Uses @noble/curves for key generation
 */

import { ed25519 } from '@noble/curves/ed25519.js';
import { sha256 } from '@noble/hashes/sha2.js';
import type { KeyPair } from './types';

/**
 * Generate a new Ed25519 key pair
 *
 * @returns KeyPair with public/private keys and derived KID
 */
export function generateKeyPair(): KeyPair {
  const { secretKey, publicKey } = ed25519.keygen();
  const kid = deriveKid(publicKey);

  return { publicKey, privateKey: secretKey, kid };
}

/**
 * Derive a Key ID from a public key
 * KID = base64url(SHA-256(pubkey)[0:16])
 *
 * @param publicKey - Ed25519 public key (32 bytes)
 * @returns Base64url-encoded KID (truncated to 16 bytes)
 */
export function deriveKid(publicKey: Uint8Array): string {
  const hash = sha256(publicKey);
  const truncated = hash.slice(0, 16);
  return encodeBase64Url(truncated);
}

/**
 * Encode bytes as base64url (RFC 4648)
 *
 * @param bytes - Bytes to encode
 * @returns Base64url string (no padding)
 */
export function encodeBase64Url(bytes: Uint8Array): string {
  const base64 = btoa(String.fromCharCode(...bytes));
  return base64.replace(/\+/g, '-').replace(/\//g, '_').replace(/=/g, '');
}
