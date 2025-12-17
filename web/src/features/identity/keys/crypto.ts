/**
 * Cryptographic operations for Ed25519 keys
 * Uses @noble/curves for signing/verification
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
 * Sign a message with an Ed25519 private key
 *
 * @param message - Message bytes to sign
 * @param privateKey - Ed25519 private key (32 bytes)
 * @returns Ed25519 signature (64 bytes)
 */
export function sign(message: Uint8Array, privateKey: Uint8Array): Uint8Array {
  return ed25519.sign(message, privateKey);
}

/**
 * Verify an Ed25519 signature
 *
 * @param message - Original message bytes
 * @param publicKey - Ed25519 public key (32 bytes)
 * @param signature - Ed25519 signature (64 bytes)
 * @returns true if signature is valid
 */
export function verify(message: Uint8Array, publicKey: Uint8Array, signature: Uint8Array): boolean {
  try {
    return ed25519.verify(signature, message, publicKey);
  } catch {
    return false;
  }
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

/**
 * Decode base64url string to bytes
 *
 * @param encoded - Base64url string
 * @returns Decoded bytes
 */
export function decodeBase64Url(encoded: string): Uint8Array {
  // Restore standard base64
  let base64 = encoded.replace(/-/g, '+').replace(/_/g, '/');

  // Add padding
  while (base64.length % 4 !== 0) {
    base64 += '=';
  }

  // Decode
  const binary = atob(base64);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) {
    bytes[i] = binary.charCodeAt(i);
  }

  return bytes;
}
