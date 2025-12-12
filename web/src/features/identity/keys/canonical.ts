/**
 * Canonical JSON serialization (RFC 8785 - JCS)
 * Matches backend serde_jcs implementation
 */

import canonicalize from 'canonicalize';
import { CryptoError } from './types';

/**
 * Canonicalize a JavaScript value to deterministic JSON bytes.
 * Uses RFC 8785 (JSON Canonicalization Scheme) for consistent serialization.
 *
 * @param value - Any JSON-serializable value
 * @returns UTF-8 encoded canonical JSON bytes
 * @throws {CryptoError} If value cannot be serialized
 */
export function canonicalizeValue(value: unknown): Uint8Array {
  try {
    const canonical = canonicalize(value);
    if (!canonical) {
      throw new CryptoError('Canonicalization returned null');
    }
    return new TextEncoder().encode(canonical);
  } catch (err) {
    throw new CryptoError(`Canonicalization failed: ${err}`);
  }
}
