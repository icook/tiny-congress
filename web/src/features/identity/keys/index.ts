/**
 * Identity key management module
 * Exports cryptographic operations, storage, and canonicalization
 */

// Types
export type { KeyPair, StoredKey, SignedEnvelope } from './types';

// Cryptographic operations
export {
  generateKeyPair,
  sign,
  verify,
  deriveKid,
  encodeBase64Url,
  decodeBase64Url,
} from './crypto';

// JSON canonicalization
export { canonicalize, canonicalizeToBytes } from './canonical';

// Key storage
export {
  keyPairToStored,
  storedToKeyPair,
  storeRootKey,
  getRootKey,
  deleteRootKey,
  storeDeviceKey,
  getDeviceKey,
  deleteDeviceKey,
  hasRootKey,
  hasDeviceKey,
  clearAllKeys,
} from './storage';
