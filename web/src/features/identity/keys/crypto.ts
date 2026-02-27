/**
 * Cryptographic operations for Ed25519 keys
 *
 * Key generation uses @noble/curves (runs locally in JS).
 * KID derivation uses the tc-crypto WASM module (shared with the Rust backend).
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
 * Sign a message with an Ed25519 private key
 *
 * @returns 64-byte signature
 */
export function signMessage(message: Uint8Array, privateKey: Uint8Array): Uint8Array {
  return ed25519.sign(message, privateKey);
}

/**
 * Build a minimal backup envelope for signup.
 *
 * In production this would encrypt the root private key with a password-derived key.
 * For now, we build a valid envelope structure with placeholder encrypted data.
 */
export function buildBackupEnvelope(): Uint8Array {
  // Fixed header layout matching BackupEnvelope::build in Rust:
  // [version:1][kdf_id:1][m_cost:4LE][t_cost:4LE][p_cost:4LE][salt:16][nonce:12][ciphertext:48+]
  const envelope = new Uint8Array(90); // minimum valid size
  envelope[0] = 0x01; // version
  envelope[1] = 0x01; // kdf_id = Argon2id

  // m_cost = 65536 (LE u32)
  const view = new DataView(envelope.buffer);
  view.setUint32(2, 65536, true);
  // t_cost = 3
  view.setUint32(6, 3, true);
  // p_cost = 1
  view.setUint32(10, 1, true);

  // salt (16 bytes) - random
  globalThis.crypto.getRandomValues(envelope.subarray(14, 30));
  // nonce (12 bytes) - random
  globalThis.crypto.getRandomValues(envelope.subarray(30, 42));
  // ciphertext (48 bytes) - placeholder
  globalThis.crypto.getRandomValues(envelope.subarray(42, 90));

  return envelope;
}
