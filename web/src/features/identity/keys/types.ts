/**
 * Type definitions for identity key management
 */

/**
 * Stored key representation (persisted in IndexedDB)
 */
export interface StoredKey {
  /** Key ID - SHA-256 hash of public key, base64url encoded */
  kid: string;
  /** Ed25519 public key, base64url encoded */
  publicKey: string;
  /** Ed25519 private key, base64url encoded */
  privateKey: string;
  /** ISO 8601 timestamp when key was created */
  createdAt: string;
  /** Optional human-readable label */
  label?: string;
}

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

/**
 * Signed envelope for cryptographic operations
 */
export interface SignedEnvelope {
  /** Arbitrary JSON payload */
  payload: Record<string, unknown>;
  /** Signer metadata */
  signer: {
    /** Key ID of signing key */
    kid: string;
    /** Optional account ID */
    account_id?: string;
    /** Optional device ID */
    device_id?: string;
  };
  /** Ed25519 signature over canonical JSON, base64url encoded */
  signature: string;
}
