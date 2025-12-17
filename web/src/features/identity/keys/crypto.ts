/**
 * Cryptographic operations for Ed25519 keys
 *
 * Key generation uses @noble/curves (runs locally in JS).
 * KID derivation and base64url encoding use the tc-crypto WASM module
 * (shared with the Rust backend for consistency).
 */

import { ed25519 } from '@noble/curves/ed25519.js';
import type { CryptoModule } from '@/providers/CryptoProvider';
import type { KeyPair } from './types';

/**
 * Generate a new Ed25519 key pair
 *
 * @param crypto - The WASM crypto module from useCryptoRequired()
 * @returns KeyPair with public/private keys and derived KID
 */
export function generateKeyPair(crypto: CryptoModule): KeyPair {
  const { secretKey, publicKey } = ed25519.keygen();
  const kid = crypto.derive_kid(publicKey);

  return { publicKey, privateKey: secretKey, kid };
}

/**
 * Encode bytes as base64url (RFC 4648)
 * Delegates to WASM for consistency with backend.
 *
 * @param crypto - The WASM crypto module from useCryptoRequired()
 * @param bytes - Bytes to encode
 * @returns Base64url string (no padding)
 */
export function encodeBase64Url(crypto: CryptoModule, bytes: Uint8Array): string {
  return crypto.encode_base64url(bytes);
}

/**
 * Derive a Key ID from a public key
 * KID = base64url(SHA-256(pubkey)[0:16])
 * Delegates to WASM for consistency with backend.
 *
 * @param crypto - The WASM crypto module from useCryptoRequired()
 * @param publicKey - Ed25519 public key (32 bytes)
 * @returns Base64url-encoded KID (truncated to 16 bytes)
 */
export function deriveKid(crypto: CryptoModule, publicKey: Uint8Array): string {
  return crypto.derive_kid(publicKey);
}
