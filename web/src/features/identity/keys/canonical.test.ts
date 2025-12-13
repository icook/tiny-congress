import { describe, expect, it } from 'vitest';
import { canonicalize, canonicalizeToBytes } from './canonical';

describe('canonical', () => {
  describe('canonicalize', () => {
    it('should canonicalize null', () => {
      expect(canonicalize(null)).toBe('null');
    });

    it('should canonicalize booleans', () => {
      expect(canonicalize(true)).toBe('true');
      expect(canonicalize(false)).toBe('false');
    });

    it('should canonicalize numbers', () => {
      expect(canonicalize(0)).toBe('0');
      expect(canonicalize(42)).toBe('42');
      expect(canonicalize(-17)).toBe('-17');
      expect(canonicalize(3.14)).toBe('3.14');
    });

    it('should canonicalize strings', () => {
      expect(canonicalize('hello')).toBe('"hello"');
      expect(canonicalize('')).toBe('""');
    });

    it('should escape special characters in strings', () => {
      expect(canonicalize('line1\nline2')).toBe('"line1\\nline2"');
      expect(canonicalize('tab\there')).toBe('"tab\\there"');
      expect(canonicalize('quote"test')).toBe('"quote\\"test"');
      expect(canonicalize('backslash\\test')).toBe('"backslash\\\\test"');
    });

    it('should canonicalize arrays', () => {
      expect(canonicalize([])).toBe('[]');
      expect(canonicalize([1, 2, 3])).toBe('[1,2,3]');
      expect(canonicalize(['a', 'b'])).toBe('["a","b"]');
      expect(canonicalize([1, 'two', true, null])).toBe('[1,"two",true,null]');
    });

    it('should canonicalize nested arrays', () => {
      expect(
        canonicalize([
          [1, 2],
          [3, 4],
        ])
      ).toBe('[[1,2],[3,4]]');
    });

    it('should canonicalize objects with sorted keys', () => {
      expect(canonicalize({})).toBe('{}');
      expect(canonicalize({ a: 1 })).toBe('{"a":1}');
      expect(canonicalize({ b: 2, a: 1 })).toBe('{"a":1,"b":2}');
    });

    it('should sort object keys lexicographically', () => {
      const obj = {
        zebra: 1,
        apple: 2,
        banana: 3,
      };
      expect(canonicalize(obj)).toBe('{"apple":2,"banana":3,"zebra":1}');
    });

    it('should canonicalize nested objects', () => {
      const obj = {
        outer: {
          b: 2,
          a: 1,
        },
      };
      expect(canonicalize(obj)).toBe('{"outer":{"a":1,"b":2}}');
    });

    it('should canonicalize complex nested structures', () => {
      const obj = {
        type: 'DeviceDelegation',
        device_id: 'abc-123',
        permissions: ['read', 'write'],
        metadata: {
          name: 'My Device',
          created: 1234567890,
        },
      };

      const expected =
        '{"device_id":"abc-123","metadata":{"created":1234567890,"name":"My Device"},"permissions":["read","write"],"type":"DeviceDelegation"}';
      expect(canonicalize(obj)).toBe(expected);
    });

    it('should produce same output for equivalent objects with different key order', () => {
      const obj1 = { a: 1, b: 2, c: 3 };
      const obj2 = { c: 3, a: 1, b: 2 };
      const obj3 = { b: 2, c: 3, a: 1 };

      const canonical1 = canonicalize(obj1);
      const canonical2 = canonicalize(obj2);
      const canonical3 = canonicalize(obj3);

      expect(canonical1).toBe(canonical2);
      expect(canonical2).toBe(canonical3);
    });

    it('should throw for non-finite numbers', () => {
      expect(() => canonicalize(Infinity)).toThrow();
      expect(() => canonicalize(-Infinity)).toThrow();
      expect(() => canonicalize(NaN)).toThrow();
    });
  });

  describe('canonicalizeToBytes', () => {
    it('should produce UTF-8 encoded bytes', () => {
      const obj = { a: 1, b: 2 };
      const bytes = canonicalizeToBytes(obj);

      // Verify it's an array-like object with correct properties
      expect(bytes).toHaveProperty('length');
      expect(bytes.length).toBeGreaterThan(0);

      // Decode and verify
      const decoded = new TextDecoder().decode(bytes);
      expect(decoded).toBe('{"a":1,"b":2}');
    });

    it('should handle Unicode characters', () => {
      const obj = { emoji: '🔐', text: 'café' };
      const bytes = canonicalizeToBytes(obj);
      const decoded = new TextDecoder().decode(bytes);

      expect(decoded).toContain('🔐');
      expect(decoded).toContain('café');
    });
  });
});
