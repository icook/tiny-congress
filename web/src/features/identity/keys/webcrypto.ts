/**
 * Web Crypto Ed25519 utilities for non-extractable device keys.
 *
 * Device signing keys use the Web Crypto API so the raw key material
 * never leaves the browser's secure key store, even if JavaScript is
 * compromised (XSS). The CryptoKey objects are stored directly in
 * IndexedDB (they're structured-cloneable).
 */

export interface WebCryptoKeyPair {
  /** Raw Ed25519 public key bytes (32 bytes, extractable) */
  publicKey: Uint8Array;
  /** Non-extractable CryptoKey handle for signing */
  privateKey: CryptoKey;
}

/**
 * Generate a new Ed25519 key pair via Web Crypto.
 * The private key is non-extractable — it can sign but its raw bytes
 * cannot be read by JavaScript.
 */
export async function generateDeviceKeyPair(): Promise<WebCryptoKeyPair> {
  const keyPair = await crypto.subtle.generateKey('Ed25519', false, ['sign']);

  // Export just the public key as raw bytes
  const publicKeyBuffer = await crypto.subtle.exportKey('raw', keyPair.publicKey);

  return {
    publicKey: new Uint8Array(publicKeyBuffer),
    privateKey: keyPair.privateKey,
  };
}

/**
 * Sign a message using a non-extractable Ed25519 CryptoKey.
 * @returns 64-byte Ed25519 signature
 */
export async function signWithDeviceKey(
  message: Uint8Array,
  privateKey: CryptoKey
): Promise<Uint8Array> {
  const signature = await crypto.subtle.sign(
    'Ed25519',
    privateKey,
    message as ArrayBufferView<ArrayBuffer>
  );
  return new Uint8Array(signature);
}
