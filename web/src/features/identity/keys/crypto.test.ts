import { ed25519 } from '@noble/curves/ed25519.js';
import { describe, expect, test, vi } from 'vitest';
import type { CryptoModule } from '@/providers/CryptoProvider';
import { buildBackupEnvelope, decryptBackupEnvelope, generateKeyPair, signMessage } from './crypto';

function mockCryptoModule(): CryptoModule {
  return {
    derive_kid: vi.fn(() => 'mock-kid'),
    encode_base64url: vi.fn(),
    decode_base64url: vi.fn(),
  };
}

describe('identity crypto utilities', () => {
  test('generateKeyPair returns valid key pair with KID from crypto module', () => {
    const crypto = mockCryptoModule();
    const keyPair = generateKeyPair(crypto);

    expect(keyPair.publicKey).toBeInstanceOf(Uint8Array);
    expect(keyPair.publicKey.length).toBe(32);
    expect(keyPair.privateKey).toBeInstanceOf(Uint8Array);
    expect(keyPair.privateKey.length).toBe(32);

    expect(crypto.derive_kid).toHaveBeenCalledWith(keyPair.publicKey);
    expect(keyPair.kid).toBe('mock-kid');
  });

  test('generateKeyPair produces unique keys', () => {
    const crypto = mockCryptoModule();
    const keyPair1 = generateKeyPair(crypto);
    const keyPair2 = generateKeyPair(crypto);

    expect(keyPair1.publicKey).not.toEqual(keyPair2.publicKey);
  });

  test('signMessage produces a valid Ed25519 signature', () => {
    const { secretKey, publicKey } = ed25519.keygen();
    const message = new TextEncoder().encode('hello');
    const signature = signMessage(message, secretKey);

    expect(signature).toBeInstanceOf(Uint8Array);
    expect(signature.length).toBe(64);

    // Verify the signature is valid
    expect(ed25519.verify(signature, message, publicKey)).toBe(true);
  });

  test('signMessage produces different signatures for different messages', () => {
    const { secretKey } = ed25519.keygen();
    const sig1 = signMessage(new TextEncoder().encode('msg-1'), secretKey);
    const sig2 = signMessage(new TextEncoder().encode('msg-2'), secretKey);

    expect(sig1).not.toEqual(sig2);
  });
});

describe('backup envelope encryption', () => {
  test('buildBackupEnvelope returns 90-byte envelope with correct header', async () => {
    const rootPrivateKey = globalThis.crypto.getRandomValues(new Uint8Array(32));
    const envelope = await buildBackupEnvelope(rootPrivateKey, 'test-password');

    expect(envelope).toBeInstanceOf(Uint8Array);
    expect(envelope.length).toBe(90);

    // Version
    expect(envelope[0]).toBe(0x01);
    // KDF ID (Argon2id)
    expect(envelope[1]).toBe(0x01);

    // m_cost = 65536 (LE u32)
    const view = new DataView(envelope.buffer, envelope.byteOffset, envelope.byteLength);
    expect(view.getUint32(2, true)).toBe(65536);
    // t_cost = 3
    expect(view.getUint32(6, true)).toBe(3);
    // p_cost = 1
    expect(view.getUint32(10, true)).toBe(1);
  });

  test('buildBackupEnvelope produces unique envelopes (random salt/nonce)', async () => {
    const rootPrivateKey = globalThis.crypto.getRandomValues(new Uint8Array(32));
    const env1 = await buildBackupEnvelope(rootPrivateKey, 'test');
    const env2 = await buildBackupEnvelope(rootPrivateKey, 'test');

    // Salt starts at offset 14, length 16 — should differ
    expect(env1.subarray(14, 30)).not.toEqual(env2.subarray(14, 30));
  });

  test('encrypt and decrypt roundtrip recovers the root key', async () => {
    const rootPrivateKey = globalThis.crypto.getRandomValues(new Uint8Array(32));
    const password = 'test-password-123';

    const envelope = await buildBackupEnvelope(rootPrivateKey, password);
    const recovered = await decryptBackupEnvelope(envelope, password);

    expect(recovered).toEqual(rootPrivateKey);
  });

  test('decrypt with wrong password throws', async () => {
    const rootPrivateKey = globalThis.crypto.getRandomValues(new Uint8Array(32));
    const envelope = await buildBackupEnvelope(rootPrivateKey, 'correct-password');

    await expect(decryptBackupEnvelope(envelope, 'wrong-password')).rejects.toThrow();
  });
});
