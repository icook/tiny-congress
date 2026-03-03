/**
 * Cryptographic operations for Ed25519 keys
 *
 * Key generation uses @noble/curves (runs locally in JS).
 * KID derivation uses the tc-crypto WASM module (shared with the Rust backend).
 * Backup encryption uses Argon2id (hash-wasm) + ChaCha20-Poly1305 (@noble/ciphers).
 */

import { chacha20poly1305 } from '@noble/ciphers/chacha.js';
import { ed25519 } from '@noble/curves/ed25519.js';
import { argon2id } from 'hash-wasm';
import type { CryptoModule } from '@/providers/CryptoProvider';
import type { KeyPair } from './types';

/**
 * Thrown when backup envelope decryption fails (wrong password or corrupted data).
 */
export class DecryptionError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'DecryptionError';
  }
}

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
 * @param message - The message bytes to sign
 * @param privateKey - The 32-byte Ed25519 private key
 * @returns 64-byte Ed25519 signature
 */
export function signMessage(message: Uint8Array, privateKey: Uint8Array): Uint8Array {
  return ed25519.sign(message, privateKey);
}

/**
 * KDF parameters matching BackupEnvelope constraints.
 * These are the minimums enforced by the Rust BackupEnvelope::parse.
 */
const KDF_M_COST = 65536; // 64 MiB
const KDF_T_COST = 3;
const KDF_P_COST = 1;
const KDF_HASH_LENGTH = 32;

/**
 * Build an encrypted backup envelope containing the root private key.
 *
 * Uses Argon2id for key derivation and ChaCha20-Poly1305 for encryption.
 * The envelope format matches the Rust BackupEnvelope binary layout:
 * [version:1][kdf_id:1][m_cost:4LE][t_cost:4LE][p_cost:4LE][salt:16][nonce:12][ciphertext:48]
 *
 * @param rootPrivateKey - 32-byte Ed25519 private key to encrypt
 * @param password - User's backup password
 * @returns Binary envelope (90 bytes: 42 header + 48 ciphertext)
 */
export async function buildBackupEnvelope(
  rootPrivateKey: Uint8Array,
  password: string
): Promise<Uint8Array> {
  const salt = globalThis.crypto.getRandomValues(new Uint8Array(16));
  const nonce = globalThis.crypto.getRandomValues(new Uint8Array(12));

  // Derive encryption key via Argon2id
  const keyBytes = await argon2id({
    password,
    salt,
    parallelism: KDF_P_COST,
    iterations: KDF_T_COST,
    memorySize: KDF_M_COST,
    hashLength: KDF_HASH_LENGTH,
    outputType: 'binary',
  });

  // Encrypt root private key with ChaCha20-Poly1305
  const cipher = chacha20poly1305(keyBytes, nonce);
  const ciphertext = cipher.encrypt(rootPrivateKey);

  // Assemble envelope
  const envelope = new Uint8Array(42 + ciphertext.length);
  const view = new DataView(envelope.buffer);
  envelope[0] = 0x01; // version
  envelope[1] = 0x01; // kdf_id = Argon2id
  view.setUint32(2, KDF_M_COST, true);
  view.setUint32(6, KDF_T_COST, true);
  view.setUint32(10, KDF_P_COST, true);
  envelope.set(salt, 14);
  envelope.set(nonce, 30);
  envelope.set(ciphertext, 42);

  return envelope;
}

/**
 * Decrypt a backup envelope to recover the root private key.
 *
 * @param envelope - Binary envelope bytes (from server)
 * @param password - User's backup password
 * @returns 32-byte Ed25519 private key
 * @throws Error if password is wrong or envelope is corrupt
 */
export async function decryptBackupEnvelope(
  envelope: Uint8Array,
  password: string
): Promise<Uint8Array> {
  if (envelope.length < 90) {
    throw new Error('Backup envelope too small');
  }
  if (envelope[0] !== 0x01) {
    throw new Error('Unsupported envelope version');
  }
  if (envelope[1] !== 0x01) {
    throw new Error('Unsupported KDF');
  }

  const view = new DataView(envelope.buffer, envelope.byteOffset, envelope.byteLength);
  const mCost = view.getUint32(2, true);
  const tCost = view.getUint32(6, true);
  const pCost = view.getUint32(10, true);

  // Enforce same KDF minimums as the Rust backend (BackupEnvelope::parse).
  // A malicious server could send a weak-KDF envelope to make brute-forcing trivial.
  if (mCost < KDF_M_COST) {
    throw new Error(`KDF m_cost ${String(mCost)} below minimum ${String(KDF_M_COST)}`);
  }
  if (tCost < KDF_T_COST) {
    throw new Error(`KDF t_cost ${String(tCost)} below minimum ${String(KDF_T_COST)}`);
  }
  if (pCost < KDF_P_COST) {
    throw new Error(`KDF p_cost ${String(pCost)} below minimum ${String(KDF_P_COST)}`);
  }

  const salt = envelope.slice(14, 30);
  const nonce = envelope.slice(30, 42);
  const ciphertext = envelope.slice(42);

  // Derive decryption key via Argon2id
  const keyBytes = await argon2id({
    password,
    salt,
    parallelism: pCost,
    iterations: tCost,
    memorySize: mCost,
    hashLength: KDF_HASH_LENGTH,
    outputType: 'binary',
  });

  // Decrypt with ChaCha20-Poly1305
  const cipher = chacha20poly1305(keyBytes, nonce);
  let plaintext: Uint8Array;
  try {
    plaintext = cipher.decrypt(ciphertext);
  } catch {
    throw new DecryptionError('Wrong password or corrupted backup');
  }

  if (plaintext.length !== 32) {
    throw new DecryptionError('Decrypted key has unexpected length');
  }

  return plaintext;
}
