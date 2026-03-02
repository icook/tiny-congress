import { describe, expect, test } from 'vitest';
import { generateDeviceKeyPair, signWithDeviceKey } from './webcrypto';

/**
 * These tests exercise the real Web Crypto Ed25519 implementation.
 * Node.js 22+ supports Ed25519 via globalThis.crypto.subtle, which jsdom
 * delegates to. If the runtime doesn't support it, the tests will fail
 * with a clear error rather than silently passing with mocks.
 */
describe('Web Crypto Ed25519', () => {
  test('generateDeviceKeyPair returns 32-byte public key and non-extractable CryptoKey', async () => {
    const keyPair = await generateDeviceKeyPair();
    expect(keyPair.publicKey).toBeInstanceOf(Uint8Array);
    expect(keyPair.publicKey.length).toBe(32);
    expect(keyPair.privateKey).toBeDefined();
    expect(keyPair.privateKey.extractable).toBe(false);
    expect(keyPair.privateKey.type).toBe('private');
  });

  test('generateDeviceKeyPair produces unique key pairs', async () => {
    const keyPair1 = await generateDeviceKeyPair();
    const keyPair2 = await generateDeviceKeyPair();
    expect(keyPair1.publicKey).not.toEqual(keyPair2.publicKey);
  });

  test('signWithDeviceKey produces valid 64-byte signature', async () => {
    const keyPair = await generateDeviceKeyPair();
    const message = new TextEncoder().encode('test message');
    const signature = await signWithDeviceKey(message, keyPair.privateKey);
    expect(signature).toBeInstanceOf(Uint8Array);
    expect(signature.length).toBe(64);

    // Round-trip: import the raw public key and verify the signature
    const publicCryptoKey = await globalThis.crypto.subtle.importKey(
      'raw',
      keyPair.publicKey as BufferSource,
      'Ed25519',
      true,
      ['verify']
    );
    const valid = await globalThis.crypto.subtle.verify(
      'Ed25519',
      publicCryptoKey,
      signature as BufferSource,
      message as BufferSource
    );
    expect(valid).toBe(true);
  });

  test('signWithDeviceKey produces deterministic signatures for same key and message', async () => {
    const keyPair = await generateDeviceKeyPair();
    const message = new TextEncoder().encode('deterministic');
    const sig1 = await signWithDeviceKey(message, keyPair.privateKey);
    const sig2 = await signWithDeviceKey(message, keyPair.privateKey);
    expect(sig1).toEqual(sig2);
  });

  test('signWithDeviceKey produces different signatures for different messages', async () => {
    const keyPair = await generateDeviceKeyPair();
    const sig1 = await signWithDeviceKey(new TextEncoder().encode('message A'), keyPair.privateKey);
    const sig2 = await signWithDeviceKey(new TextEncoder().encode('message B'), keyPair.privateKey);
    expect(sig1).not.toEqual(sig2);
  });

  test('signature from one key pair fails verification with different public key', async () => {
    const keyPairA = await generateDeviceKeyPair();
    const keyPairB = await generateDeviceKeyPair();
    const message = new TextEncoder().encode('cross-key test');
    const signatureA = await signWithDeviceKey(message, keyPairA.privateKey);

    const publicKeyB = await globalThis.crypto.subtle.importKey(
      'raw',
      keyPairB.publicKey as BufferSource,
      'Ed25519',
      true,
      ['verify']
    );
    const valid = await globalThis.crypto.subtle.verify(
      'Ed25519',
      publicKeyB,
      signatureA as BufferSource,
      message as BufferSource
    );
    expect(valid).toBe(false);
  });
});
