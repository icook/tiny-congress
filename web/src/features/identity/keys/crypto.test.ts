import { describe, expect, test, vi } from 'vitest';
import type { CryptoModule } from '@/providers/CryptoProvider';
import { generateKeyPair } from './crypto';

describe('identity crypto utilities', () => {
  test('generateKeyPair returns valid key pair with KID from crypto module', () => {
    const mockCrypto: CryptoModule = {
      derive_kid: vi.fn(() => 'mock-kid'),
      encode_base64url: vi.fn(),
      decode_base64url: vi.fn(),
    };

    const keyPair = generateKeyPair(mockCrypto);

    // Key pair should have valid lengths
    expect(keyPair.publicKey).toBeInstanceOf(Uint8Array);
    expect(keyPair.publicKey.length).toBe(32);
    expect(keyPair.privateKey).toBeInstanceOf(Uint8Array);
    expect(keyPair.privateKey.length).toBe(32); // Ed25519 secret key is 32 bytes

    // KID should come from the crypto module
    expect(mockCrypto.derive_kid).toHaveBeenCalledWith(keyPair.publicKey);
    expect(keyPair.kid).toBe('mock-kid');
  });

  test('generateKeyPair produces unique keys', () => {
    const mockCrypto: CryptoModule = {
      derive_kid: vi.fn(() => 'mock-kid'),
      encode_base64url: vi.fn(),
      decode_base64url: vi.fn(),
    };

    const keyPair1 = generateKeyPair(mockCrypto);
    const keyPair2 = generateKeyPair(mockCrypto);

    // Public keys should be different
    expect(keyPair1.publicKey).not.toEqual(keyPair2.publicKey);
  });
});
