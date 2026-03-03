/**
 * Identity API client
 * Type-safe REST client for identity endpoints
 */

import { fetchJson } from '@/api/fetchClient';
import { signedFetchJson } from '@/api/signing';
import type { CryptoModule } from '@/providers/CryptoProvider';

// Re-export for backward compatibility (tests import from here)
export { fetchJson, signedFetchJson };

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
