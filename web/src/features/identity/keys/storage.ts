/**
 * Persistent key storage in IndexedDB
 * Uses idb-keyval for simple key-value operations
 */

import { get, set, del } from 'idb-keyval';
import type { KeyPair, StoredKey } from './types';
import { decodeBase64Url, encodeBase64Url } from './crypto';

const ROOT_KEY_STORAGE_KEY = 'identity:root-key';
const DEVICE_KEY_STORAGE_KEY = 'identity:device-key';

/**
 * Convert runtime KeyPair to StoredKey format
 *
 * @param keyPair - Runtime key pair
 * @param label - Optional human-readable label
 * @returns Storable key representation
 */
export function keyPairToStored(keyPair: KeyPair, label?: string): StoredKey {
  return {
    kid: keyPair.kid,
    publicKey: encodeBase64Url(keyPair.publicKey),
    privateKey: encodeBase64Url(keyPair.privateKey),
    createdAt: new Date().toISOString(),
    label,
  };
}

/**
 * Convert StoredKey to runtime KeyPair format
 *
 * @param stored - Stored key representation
 * @returns Runtime key pair
 */
export function storedToKeyPair(stored: StoredKey): KeyPair {
  return {
    kid: stored.kid,
    publicKey: decodeBase64Url(stored.publicKey),
    privateKey: decodeBase64Url(stored.privateKey),
  };
}

/**
 * Store root key in IndexedDB
 *
 * @param key - Root key to store
 */
export async function storeRootKey(key: StoredKey): Promise<void> {
  await set(ROOT_KEY_STORAGE_KEY, key);
}

/**
 * Retrieve root key from IndexedDB
 *
 * @returns Root key if stored, undefined otherwise
 */
export async function getRootKey(): Promise<StoredKey | undefined> {
  return get<StoredKey>(ROOT_KEY_STORAGE_KEY);
}

/**
 * Delete root key from IndexedDB
 */
export async function deleteRootKey(): Promise<void> {
  await del(ROOT_KEY_STORAGE_KEY);
}

/**
 * Store device key in IndexedDB
 *
 * @param key - Device key to store
 */
export async function storeDeviceKey(key: StoredKey): Promise<void> {
  await set(DEVICE_KEY_STORAGE_KEY, key);
}

/**
 * Retrieve device key from IndexedDB
 *
 * @returns Device key if stored, undefined otherwise
 */
export async function getDeviceKey(): Promise<StoredKey | undefined> {
  return get<StoredKey>(DEVICE_KEY_STORAGE_KEY);
}

/**
 * Delete device key from IndexedDB
 */
export async function deleteDeviceKey(): Promise<void> {
  await del(DEVICE_KEY_STORAGE_KEY);
}

/**
 * Check if root key exists in storage
 *
 * @returns true if root key is stored
 */
export async function hasRootKey(): Promise<boolean> {
  const key = await getRootKey();
  return key !== undefined;
}

/**
 * Check if device key exists in storage
 *
 * @returns true if device key is stored
 */
export async function hasDeviceKey(): Promise<boolean> {
  const key = await getDeviceKey();
  return key !== undefined;
}

/**
 * Clear all stored keys (logout)
 */
export async function clearAllKeys(): Promise<void> {
  await Promise.all([deleteRootKey(), deleteDeviceKey()]);
}
