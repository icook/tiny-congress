/**
 * Tests for the crypto module using WASM
 *
 * These tests verify that the WASM crypto functions produce correct output
 * and match the Rust backend implementation (cross-language compatibility).
 */

import * as fs from 'node:fs';
import * as path from 'node:path';
import { beforeAll, describe, expect, it } from 'vitest';

// We need to import and initialize the WASM module before tests
let wasmModule: typeof import('@/wasm/tc-crypto/tc_crypto.js');

beforeAll(async () => {
  wasmModule = await import('@/wasm/tc-crypto/tc_crypto.js');

  // In Node.js test environment, load WASM binary from filesystem
  // (the default fetch-based loading doesn't work in jsdom)
  const wasmPath = path.resolve(__dirname, '../../../../wasm/tc-crypto/tc_crypto_bg.wasm');
  const wasmBytes = fs.readFileSync(wasmPath);
  wasmModule.initSync({ module: wasmBytes });
});

describe('tc-crypto WASM module', () => {
  describe('derive_kid', () => {
    it('produces deterministic output', () => {
      const pubkey = new Uint8Array(32).fill(1);
      const kid1 = wasmModule.derive_kid(pubkey);
      const kid2 = wasmModule.derive_kid(pubkey);
      expect(kid1).toBe(kid2);
    });

    it('produces correct KID length', () => {
      const pubkey = new Uint8Array(32).fill(0);
      const kid = wasmModule.derive_kid(pubkey);
      // 16 bytes -> ~22 base64url chars (without padding)
      expect(kid.length).toBeGreaterThanOrEqual(21);
      expect(kid.length).toBeLessThanOrEqual(22);
    });

    it('matches Rust backend test vector', () => {
      // Test vector: all-ones pubkey
      // This MUST match the expected value in crates/tc-crypto/src/lib.rs
      const pubkey = new Uint8Array(32).fill(1);
      const kid = wasmModule.derive_kid(pubkey);
      expect(kid).toBe('cs1uhCLEB_ttCYaQ8RMLfQ');
    });
  });

  describe('encode_base64url', () => {
    it('encodes "Hello" correctly', () => {
      const bytes = new TextEncoder().encode('Hello');
      const encoded = wasmModule.encode_base64url(bytes);
      expect(encoded).toBe('SGVsbG8');
    });

    it('produces no padding', () => {
      const bytes = new Uint8Array([1, 2, 3]);
      const encoded = wasmModule.encode_base64url(bytes);
      expect(encoded).not.toContain('=');
    });

    it('uses URL-safe characters', () => {
      // Use bytes that would produce + and / in standard base64
      const bytes = new Uint8Array([251, 239]);
      const encoded = wasmModule.encode_base64url(bytes);
      expect(encoded).not.toContain('+');
      expect(encoded).not.toContain('/');
    });
  });

  describe('decode_base64url', () => {
    it('decodes "Hello" correctly', () => {
      const decoded = wasmModule.decode_base64url('SGVsbG8');
      const text = new TextDecoder().decode(decoded);
      expect(text).toBe('Hello');
    });

    it('round-trips correctly', () => {
      const original = new Uint8Array([1, 2, 3, 4, 5, 255, 254, 253]);
      const encoded = wasmModule.encode_base64url(original);
      const decoded = wasmModule.decode_base64url(encoded);
      expect(decoded).toEqual(original);
    });
  });
});
