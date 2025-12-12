/**
 * Comprehensive tests for key management module
 * Validates against backend test vectors
 */

import { describe, expect, it } from 'vitest';
import { canonicalizeValue } from './canonical';
import { deriveKid } from './kid';
import { generateDeviceKey, generateRootKey } from './rootKey';
import { signEnvelope, verifyEnvelope } from './signer';
import {
  TEST_CANONICAL_BYTES,
  TEST_ENVELOPE_PAYLOAD,
  TEST_ENVELOPE_SIGNATURE,
  TEST_KID,
  TEST_PUBLIC_KEY,
  TEST_SECRET_KEY,
} from './testVectors';
import { CryptoError } from './types';
import { decodeBase64Url, encodeBase64Url } from './utils';

describe('Key Management', () => {
  describe('Base64url encoding', () => {
    it('should encode and decode round-trip', () => {
      const original = new Uint8Array([1, 2, 3, 4, 5]);
      const encoded = encodeBase64Url(original);
      const decoded = decodeBase64Url(encoded);
      expect(decoded).toEqual(original);
    });

    it('should produce URL-safe output without padding', () => {
      const bytes = new Uint8Array(32).fill(255);
      const encoded = encodeBase64Url(bytes);
      expect(encoded).not.toContain('+');
      expect(encoded).not.toContain('/');
      expect(encoded).not.toContain('=');
    });
  });

  describe('Kid derivation', () => {
    it('should match backend test vector', () => {
      const kid = deriveKid(TEST_PUBLIC_KEY);
      expect(kid).toBe(TEST_KID);
    });

    it('should be deterministic', () => {
      const kid1 = deriveKid(TEST_PUBLIC_KEY);
      const kid2 = deriveKid(TEST_PUBLIC_KEY);
      expect(kid1).toBe(kid2);
    });

    it('should produce different kids for different keys', () => {
      const key1 = new Uint8Array(32).fill(1);
      const key2 = new Uint8Array(32).fill(2);
      const kid1 = deriveKid(key1);
      const kid2 = deriveKid(key2);
      expect(kid1).not.toBe(kid2);
    });
  });

  describe('Canonical JSON', () => {
    it('should canonicalize consistently', () => {
      const value = { b: 2, a: 1 };
      const first = canonicalizeValue(value);
      const second = canonicalizeValue(value);
      expect(first).toEqual(second);
    });

    it('should sort object keys', () => {
      const value = { z: 1, a: 2, m: 3 };
      const canonical = canonicalizeValue(value);
      const str = new TextDecoder().decode(canonical);
      expect(str).toBe('{"a":2,"m":3,"z":1}');
    });

    it('should match backend canonical bytes', () => {
      const envelope = {
        payload_type: 'Test',
        payload: TEST_ENVELOPE_PAYLOAD,
        signer: {
          account_id: null,
          device_id: null,
          kid: TEST_KID,
        },
      };

      const canonical = canonicalizeValue(envelope);
      const expected = TEST_CANONICAL_BYTES;

      expect(new TextDecoder().decode(canonical)).toBe(new TextDecoder().decode(expected));
    });

    it('should handle nested objects', () => {
      const value = {
        outer: {
          inner: {
            deep: 'value',
          },
        },
      };
      const canonical = canonicalizeValue(value);
      expect(canonical.length).toBeGreaterThan(0);
      expect(canonical[0]).toBe(123); // '{'
    });
  });

  describe('Envelope signing', () => {
    it('should match backend signature', () => {
      // Note: Backend test uses null for account_id/device_id, but TypeScript optional
      // fields are undefined. To match backend, we need to explicitly set nulls.
      const backendStyleSigner = {
        account_id: null,
        device_id: null,
        kid: TEST_KID,
      };
      const signedWithNulls = signEnvelope(
        'Test',
        TEST_ENVELOPE_PAYLOAD,
        backendStyleSigner,
        TEST_SECRET_KEY
      );

      expect(signedWithNulls.sig).toBe(TEST_ENVELOPE_SIGNATURE);
    });

    it('should verify valid envelope', () => {
      const envelope = signEnvelope(
        'Test',
        { body: 'test data' },
        {
          kid: deriveKid(TEST_PUBLIC_KEY),
        },
        TEST_SECRET_KEY
      );

      expect(() => verifyEnvelope(envelope, TEST_PUBLIC_KEY)).not.toThrow();
    });

    it('should reject envelope with wrong kid', () => {
      const envelope = signEnvelope(
        'Test',
        { body: 'test data' },
        { kid: 'wrong-kid' },
        TEST_SECRET_KEY
      );

      expect(() => verifyEnvelope(envelope, TEST_PUBLIC_KEY)).toThrow(CryptoError);
      expect(() => verifyEnvelope(envelope, TEST_PUBLIC_KEY)).toThrow('Kid mismatch');
    });

    it('should reject tampered payload', () => {
      const envelope = signEnvelope(
        'Test',
        { body: { foo: 'bar' } },
        { kid: deriveKid(TEST_PUBLIC_KEY) },
        TEST_SECRET_KEY
      );

      // Tamper payload after signing
      envelope.payload = { body: { foo: 'tampered' } };

      expect(() => verifyEnvelope(envelope, TEST_PUBLIC_KEY)).toThrow(CryptoError);
      expect(() => verifyEnvelope(envelope, TEST_PUBLIC_KEY)).toThrow(
        'Signature verification failed'
      );
    });

    it('should reject tampered signature', () => {
      const envelope = signEnvelope(
        'Test',
        { body: 'test' },
        { kid: deriveKid(TEST_PUBLIC_KEY) },
        TEST_SECRET_KEY
      );

      // Tamper signature
      const sigBytes = decodeBase64Url(envelope.sig);
      sigBytes[0] ^= 1; // Flip one bit
      envelope.sig = encodeBase64Url(sigBytes);

      expect(() => verifyEnvelope(envelope, TEST_PUBLIC_KEY)).toThrow(CryptoError);
    });
  });

  describe('Key generation', () => {
    it('should generate valid root key pair', () => {
      const keyPair = generateRootKey();
      expect(keyPair.privateKey).toHaveLength(32);
      expect(keyPair.publicKey).toHaveLength(32);
    });

    it('should generate unique keys', () => {
      const key1 = generateRootKey();
      const key2 = generateRootKey();
      expect(key1.privateKey).not.toEqual(key2.privateKey);
      expect(key1.publicKey).not.toEqual(key2.publicKey);
    });

    it('should generate valid device key', () => {
      const deviceKey = generateDeviceKey('Test Device');
      expect(deviceKey.kid).toBeTruthy();
      expect(deviceKey.publicKey).toBeTruthy();
      expect(deviceKey.privateKey).toBeTruthy();
      expect(deviceKey.label).toBe('Test Device');
      expect(new Date(deviceKey.createdAt)).toBeInstanceOf(Date);
    });

    it('should generate device key without label', () => {
      const deviceKey = generateDeviceKey();
      expect(deviceKey.kid).toBeTruthy();
      expect(deviceKey.label).toBeUndefined();
    });

    it('should generate working keypair for signing', () => {
      const { publicKey, privateKey } = generateRootKey();
      const kid = deriveKid(publicKey);

      const envelope = signEnvelope('Test', { data: 'test' }, { kid }, privateKey);

      expect(() => verifyEnvelope(envelope, publicKey)).not.toThrow();
    });
  });

  describe('Canonical order edge cases', () => {
    it('should handle empty objects', () => {
      const canonical = canonicalizeValue({});
      expect(new TextDecoder().decode(canonical)).toBe('{}');
    });

    it('should handle arrays', () => {
      const canonical = canonicalizeValue([3, 1, 2]);
      expect(new TextDecoder().decode(canonical)).toBe('[3,1,2]');
    });

    it('should handle null values', () => {
      const canonical = canonicalizeValue({ value: null });
      expect(new TextDecoder().decode(canonical)).toBe('{"value":null}');
    });

    it('should handle mixed types', () => {
      const value = {
        string: 'text',
        number: 42,
        boolean: true,
        null: null,
        array: [1, 2],
        object: { nested: 'value' },
      };
      const canonical = canonicalizeValue(value);
      expect(canonical.length).toBeGreaterThan(0);
      expect(canonical[0]).toBe(123); // '{'
    });
  });
});
