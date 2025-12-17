/**
 * Type definitions for identity key management
 */

/**
 * Runtime key pair representation (raw bytes)
 */
export interface KeyPair {
  /** Ed25519 public key (32 bytes) */
  publicKey: Uint8Array;
  /** Ed25519 private key (32 bytes) */
  privateKey: Uint8Array;
  /** Key ID - SHA-256 hash of public key, base64url encoded */
  kid: string;
}
