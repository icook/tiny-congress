/**
 * Shared request signing utilities for authenticated API calls.
 *
 * Constructs Ed25519 signatures over a canonical request representation
 * using the device's non-extractable CryptoKey.
 */

import type { CryptoModule } from '@/providers/CryptoProvider';
import { fetchJson } from './fetchClient';

async function sha256Hex(data: Uint8Array): Promise<string> {
  const hash = await globalThis.crypto.subtle.digest(
    'SHA-256',
    data as ArrayBufferView<ArrayBuffer>
  );
  return Array.from(new Uint8Array(hash))
    .map((b) => b.toString(16).padStart(2, '0'))
    .join('');
}

async function buildAuthHeaders(
  method: string,
  path: string,
  bodyBytes: Uint8Array,
  deviceKid: string,
  privateKey: CryptoKey,
  wasmCrypto: CryptoModule
): Promise<Record<string, string>> {
  const timestamp = Math.floor(Date.now() / 1000).toString();
  const nonce = globalThis.crypto.randomUUID();
  const bodyHash = await sha256Hex(bodyBytes);
  const canonical = `${method}\n${path}\n${timestamp}\n${nonce}\n${bodyHash}`;

  const signatureBuffer = await globalThis.crypto.subtle.sign(
    'Ed25519',
    privateKey,
    new TextEncoder().encode(canonical) as ArrayBufferView<ArrayBuffer>
  );

  return {
    'X-Device-Kid': deviceKid,
    'X-Signature': wasmCrypto.encode_base64url(new Uint8Array(signatureBuffer)),
    'X-Timestamp': timestamp,
    'X-Nonce': nonce,
  };
}

export async function signedFetchJson<T>(
  path: string,
  method: string,
  deviceKid: string,
  privateKey: CryptoKey,
  wasmCrypto: CryptoModule,
  body?: unknown
): Promise<T> {
  const bodyStr = body !== undefined ? JSON.stringify(body) : '';
  const bodyBytes = new TextEncoder().encode(bodyStr);
  const authHeaders = await buildAuthHeaders(
    method,
    path,
    bodyBytes,
    deviceKid,
    privateKey,
    wasmCrypto
  );

  const options: RequestInit = {
    method,
    headers: authHeaders,
  };

  if (body !== undefined) {
    options.body = bodyStr;
  }

  return fetchJson<T>(path, options);
}
