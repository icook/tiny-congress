/**
 * Identity key management types matching backend crypto module
 */

export interface EnvelopeSigner {
  account_id?: string | null;
  device_id?: string | null;
  kid: string;
}

export interface SignedEnvelope {
  v: number;
  payload_type: string;
  payload: unknown;
  signer: EnvelopeSigner;
  sig: string;
}

export interface KeyPair {
  publicKey: Uint8Array;
  privateKey: Uint8Array;
}

export interface StoredKey {
  kid: string;
  publicKey: string; // base64url
  privateKey: string; // base64url (encrypted in production)
  createdAt: string;
  label?: string;
}

export class CryptoError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'CryptoError';
  }
}
