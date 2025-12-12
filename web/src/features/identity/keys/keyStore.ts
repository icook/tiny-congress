/**
 * Key storage using IndexedDB
 * Provides persistent storage for device keys with account/device namespacing
 */

import { del, get, set } from 'idb-keyval';
import { CryptoError, type StoredKey } from './types';
import { decodeBase64Url } from './utils';

/**
 * Generate a storage key for namespacing
 */
function storageKey(accountId: string, keyType: 'device' | 'root'): string {
  return `tinycongress:${accountId}:${keyType}`;
}

/**
 * Store a device key in IndexedDB.
 * In production, this should encrypt the private key with a passphrase or WebCrypto.
 *
 * WARNING: Current implementation stores keys in plaintext for demo purposes.
 * Production must use encryption (WebCrypto AES-GCM with user passphrase).
 *
 * @param accountId - Account UUID
 * @param key - Key to store
 */
export async function storeDeviceKey(accountId: string, key: StoredKey): Promise<void> {
  const keyName = storageKey(accountId, 'device');
  await set(keyName, key);
}

/**
 * Retrieve a device key from IndexedDB.
 *
 * @param accountId - Account UUID
 * @returns Stored key or null if not found
 */
export async function getDeviceKey(accountId: string): Promise<StoredKey | null> {
  const keyName = storageKey(accountId, 'device');
  const stored = await get<StoredKey>(keyName);
  return stored || null;
}

/**
 * Delete a device key from IndexedDB.
 *
 * @param accountId - Account UUID
 */
export async function deleteDeviceKey(accountId: string): Promise<void> {
  const keyName = storageKey(accountId, 'device');
  await del(keyName);
}

/**
 * Store root key temporarily (for recovery kit export).
 * Root key should NOT be kept in storage permanently - only during signup/recovery flows.
 *
 * @param accountId - Account UUID
 * @param key - Key to store
 */
export async function storeRootKeyTemporary(accountId: string, key: StoredKey): Promise<void> {
  const keyName = storageKey(accountId, 'root');
  await set(keyName, key);
}

/**
 * Get root key (if temporarily stored).
 *
 * @param accountId - Account UUID
 * @returns Stored key or null
 */
export async function getRootKeyTemporary(accountId: string): Promise<StoredKey | null> {
  const keyName = storageKey(accountId, 'root');
  const stored = await get<StoredKey>(keyName);
  return stored || null;
}

/**
 * Delete root key from storage.
 * Should be called after recovery kit export or when logging out.
 *
 * @param accountId - Account UUID
 */
export async function deleteRootKey(accountId: string): Promise<void> {
  const keyName = storageKey(accountId, 'root');
  await del(keyName);
}

/**
 * Get device private key bytes from storage.
 *
 * @param accountId - Account UUID
 * @returns Private key bytes
 * @throws {CryptoError} If key not found
 */
export async function getDevicePrivateKey(accountId: string): Promise<Uint8Array> {
  const key = await getDeviceKey(accountId);
  if (!key) {
    throw new CryptoError('Device key not found in storage');
  }
  return decodeBase64Url(key.privateKey);
}

/**
 * Get device public key bytes from storage.
 *
 * @param accountId - Account UUID
 * @returns Public key bytes
 * @throws {CryptoError} If key not found
 */
export async function getDevicePublicKey(accountId: string): Promise<Uint8Array> {
  const key = await getDeviceKey(accountId);
  if (!key) {
    throw new CryptoError('Device key not found in storage');
  }
  return decodeBase64Url(key.publicKey);
}

/**
 * Export device public key as base64url string.
 *
 * @param accountId - Account UUID
 * @returns Base64url-encoded public key
 * @throws {CryptoError} If key not found
 */
export async function exportDevicePublicKey(accountId: string): Promise<string> {
  const key = await getDeviceKey(accountId);
  if (!key) {
    throw new CryptoError('Device key not found in storage');
  }
  return key.publicKey;
}

/**
 * Check if device key exists in storage.
 *
 * @param accountId - Account UUID
 * @returns True if key exists
 */
export async function hasDeviceKey(accountId: string): Promise<boolean> {
  const key = await getDeviceKey(accountId);
  return key !== null;
}
