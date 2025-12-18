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
