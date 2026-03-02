/**
 * Identity API client
 * Type-safe REST client for identity endpoints
 */

import { getApiBaseUrl } from '@/config';
import type { CryptoModule } from '@/providers/CryptoProvider';
import { signWithDeviceKey } from '../keys';

// === Types ===

export interface SignupBackup {
  encrypted_blob: string; // base64url
}

export interface SignupDevice {
  pubkey: string; // base64url
  name: string;
  certificate: string; // base64url
}

export interface SignupRequest {
  username: string;
  root_pubkey: string; // base64url
  backup: SignupBackup;
  device: SignupDevice;
}

export interface SignupResponse {
  account_id: string; // UUID
  root_kid: string;
  device_kid: string;
}

export interface DeviceInfo {
  device_kid: string;
  device_name: string;
  created_at: string;
  last_used_at: string | null;
  revoked_at: string | null;
}

export interface DeviceListResponse {
  devices: DeviceInfo[];
}

export interface AddDeviceResponse {
  device_kid: string;
  created_at: string;
}

export interface BackupResponse {
  encrypted_backup: string; // base64url
  root_kid: string;
}

export interface LoginDevice {
  pubkey: string; // base64url
  name: string;
  certificate: string; // base64url
}

export interface LoginRequest {
  username: string;
  device: LoginDevice;
}

export interface LoginResponse {
  account_id: string; // UUID
  root_kid: string;
  device_kid: string;
}

interface ApiErrorResponse {
  error?: string;
}

// === API Functions ===

export async function fetchJson<T>(path: string, options?: RequestInit): Promise<T> {
  const url = `${getApiBaseUrl()}${path}`;

  const merged = new Headers({ 'Content-Type': 'application/json' });
  if (options?.headers) {
    new Headers(options.headers).forEach((value, key) => {
      merged.set(key, value);
    });
  }

  const response = await fetch(url, {
    ...options,
    headers: merged,
  });

  if (!response.ok) {
    let errorBody: ApiErrorResponse = { error: 'Unknown error' };
    try {
      errorBody = (await response.json()) as ApiErrorResponse;
    } catch {
      // JSON parsing failed, use default error
    }
    const errorMessage =
      errorBody.error ?? `HTTP ${String(response.status)}: ${response.statusText}`;
    throw new Error(errorMessage);
  }

  // 204 No Content has no body
  if (response.status === 204) {
    return undefined as T;
  }

  return response.json() as Promise<T>;
}

// === Signed Request Helper ===

async function sha256Hex(data: Uint8Array): Promise<string> {
  const hash = await globalThis.crypto.subtle.digest(
    'SHA-256',
    data as ArrayBufferView<ArrayBuffer>
  );
  return Array.from(new Uint8Array(hash))
    .map((b) => b.toString(16).padStart(2, '0'))
    .join('');
}

/**
 * Build auth headers for signed device requests.
 * Uses the non-extractable CryptoKey via Web Crypto for signing.
 * Includes X-Nonce for replay prevention.
 */
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
  const signature = await signWithDeviceKey(new TextEncoder().encode(canonical), privateKey);

  return {
    'X-Device-Kid': deviceKid,
    'X-Signature': wasmCrypto.encode_base64url(signature),
    'X-Timestamp': timestamp,
    'X-Nonce': nonce,
  };
}

/**
 * Fetch JSON with device-signed authentication headers.
 */
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

// === Auth ===

export async function signup(request: SignupRequest): Promise<SignupResponse> {
  return fetchJson('/auth/signup', {
    method: 'POST',
    body: JSON.stringify(request),
  });
}

// === Device Management ===

export async function listDevices(
  deviceKid: string,
  privateKey: CryptoKey,
  wasmCrypto: CryptoModule
): Promise<DeviceListResponse> {
  return signedFetchJson('/auth/devices', 'GET', deviceKid, privateKey, wasmCrypto);
}

export async function revokeDevice(
  targetKid: string,
  deviceKid: string,
  privateKey: CryptoKey,
  wasmCrypto: CryptoModule
): Promise<void> {
  return signedFetchJson(`/auth/devices/${targetKid}`, 'DELETE', deviceKid, privateKey, wasmCrypto);
}

export async function renameDevice(
  targetKid: string,
  name: string,
  deviceKid: string,
  privateKey: CryptoKey,
  wasmCrypto: CryptoModule
): Promise<void> {
  return signedFetchJson(`/auth/devices/${targetKid}`, 'PATCH', deviceKid, privateKey, wasmCrypto, {
    name,
  });
}

// === Login / Backup ===

export async function fetchBackup(username: string): Promise<BackupResponse> {
  return fetchJson(`/auth/backup/${encodeURIComponent(username)}`, {
    method: 'GET',
  });
}

export async function login(request: LoginRequest): Promise<LoginResponse> {
  return fetchJson('/auth/login', {
    method: 'POST',
    body: JSON.stringify(request),
  });
}
