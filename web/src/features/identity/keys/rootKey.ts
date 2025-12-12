/**
 * Root key generation and management
 * Root keys are used to sign device delegations and should be kept secure
 */

import { ed25519 } from '@noble/curves/ed25519.js';
import { deriveKid } from './kid';
import type { KeyPair, StoredKey } from './types';
import { encodeBase64Url } from './utils';

/**
 * Generate a new Ed25519 root key pair.
 * Uses cryptographically secure random number generation.
 *
 * @returns Key pair with 32-byte public and private keys
 */
export function generateRootKey(): KeyPair {
  const { secretKey, publicKey } = ed25519.keygen();

  return {
    privateKey: secretKey,
    publicKey,
  };
}

/**
 * Generate a new Ed25519 device key pair.
 * Uses cryptographically secure random number generation.
 *
 * @param label - Optional label for the device (e.g., "iPhone 12")
 * @returns Stored key ready for persistence
 */
export function generateDeviceKey(label?: string): StoredKey {
  const { secretKey, publicKey } = ed25519.keygen();
  const kid = deriveKid(publicKey);

  return {
    kid,
    publicKey: encodeBase64Url(publicKey),
    privateKey: encodeBase64Url(secretKey),
    createdAt: new Date().toISOString(),
    label,
  };
}

/**
 * Derive kid from a public key.
 * Exposed for convenience - wraps kid.ts module.
 *
 * @param publicKey - Ed25519 public key bytes
 * @returns Base64url-encoded SHA-256 hash
 */
export function getKidFromPublicKey(publicKey: Uint8Array): string {
  return deriveKid(publicKey);
}

/**
 * Export root key secret for recovery kit.
 * Returns both public and private keys as base64url for offline backup.
 *
 * WARNING: Handle with care - private key exposure compromises account security.
 *
 * @param keyPair - Root key pair
 * @returns Object with kid, public key, and private key (all base64url)
 */
export function exportRootSecret(keyPair: KeyPair): {
  kid: string;
  publicKey: string;
  privateKey: string;
} {
  const kid = deriveKid(keyPair.publicKey);
  return {
    kid,
    publicKey: encodeBase64Url(keyPair.publicKey),
    privateKey: encodeBase64Url(keyPair.privateKey),
  };
}

/**
 * Import root key from recovery kit.
 *
 * @param privateKeyBase64 - Base64url-encoded private key
 * @returns Key pair
 */
export function importRootKey(privateKeyBase64: string): KeyPair {
  // Decode base64url
  const privateKey = Uint8Array.from(
    atob(privateKeyBase64.replace(/-/g, '+').replace(/_/g, '/')),
    (c) => c.charCodeAt(0)
  );
  const publicKey = ed25519.getPublicKey(privateKey);

  return {
    privateKey,
    publicKey,
  };
}
