/**
 * Identity key management module
 * Public API for Ed25519 key generation, storage, and signing
 */

// Re-export types
export type { EnvelopeSigner, KeyPair, SignedEnvelope, StoredKey } from './types';
export { CryptoError } from './types';

// Re-export utilities
export { decodeBase64Url, encodeBase64Url } from './utils';

// Re-export kid derivation
export { deriveKid } from './kid';

// Re-export canonicalization
export { canonicalizeValue } from './canonical';

// Re-export signing
export { signChallenge, signEnvelope, verifyEnvelope } from './signer';

// Re-export key generation
export {
  exportRootSecret,
  generateDeviceKey,
  generateRootKey,
  getKidFromPublicKey,
  importRootKey,
} from './rootKey';

// Re-export key storage
export {
  deleteDeviceKey,
  deleteRootKey,
  exportDevicePublicKey,
  getDeviceKey,
  getDevicePrivateKey,
  getDevicePublicKey,
  getRootKeyTemporary,
  hasDeviceKey,
  storeDeviceKey,
  storeRootKeyTemporary,
} from './keyStore';
