/**
 * Identity API client
 * Type-safe REST client for identity endpoints
 */

import type { SignedEnvelope } from '../keys';

// === Base Configuration ===
const API_BASE_URL = import.meta.env.VITE_API_URL || 'http://localhost:3000';

// === Types ===

// Shared
export interface DeviceMetadata {
  name?: string;
  type?: string;
}

// Auth - Signup
export interface SignupRequest {
  username: string;
  root_pubkey: string; // base64url
  device_pubkey: string; // base64url
  device_metadata?: DeviceMetadata;
  delegation_envelope: SignedEnvelope;
}

export interface SignupResponse {
  account_id: string; // UUID
  device_id: string; // UUID
  root_kid: string;
}

// Auth - Challenge/Login
export interface ChallengeRequest {
  account_id: string;
  device_id: string;
}

export interface ChallengeResponse {
  challenge_id: string;
  nonce: string; // base64url
  expires_at: string; // ISO datetime
}

export interface VerifyRequest {
  challenge_id: string;
  account_id: string;
  device_id: string;
  signature: string; // base64url
}

export interface VerifyResponse {
  session_id: string;
  expires_at: string;
}

// Devices
export interface AddDeviceRequest {
  account_id: string;
  device_pubkey: string;
  device_metadata?: DeviceMetadata;
  delegation_envelope: SignedEnvelope;
}

export interface AddDeviceResponse {
  device_id: string;
  device_kid: string;
}

export interface RevokeDeviceRequest {
  account_id: string;
  delegation_envelope: SignedEnvelope; // Contains device_id in payload
}

// Endorsements
export interface EndorsementCreateRequest {
  account_id: string;
  device_id: string;
  envelope: SignedEnvelope;
}

export interface EndorsementCreateResponse {
  endorsement_id: string;
}

export interface EndorsementRevokeRequest {
  account_id: string;
  device_id: string;
  envelope: SignedEnvelope;
}

// Recovery
export interface RecoveryHelper {
  helper_account_id: string;
  helper_root_kid?: string;
}

export interface RecoveryPolicyRequest {
  account_id: string;
  envelope: SignedEnvelope;
}

export interface RecoveryPolicyResponse {
  policy_id: string;
  threshold: number;
  helpers: RecoveryHelper[];
}

export interface RecoveryPolicyView {
  policy_id: string;
  threshold: number;
  helpers: RecoveryHelper[];
  created_at: string;
  revoked_at?: string;
}

export interface RecoveryApprovalRequest {
  account_id: string;
  helper_account_id: string;
  helper_device_id: string;
  policy_id: string;
  envelope: SignedEnvelope;
}

export interface RecoveryApprovalResponse {
  approval_id: string;
}

export interface RootRotationRequest {
  account_id: string;
  envelope: SignedEnvelope;
}

export interface RootRotationResponse {
  new_root_kid: string;
}

// === API Functions ===

async function fetchJson<T>(
  path: string,
  options?: RequestInit
): Promise<T> {
  const url = `${API_BASE_URL}${path}`;

  const response = await fetch(url, {
    ...options,
    headers: {
      'Content-Type': 'application/json',
      ...options?.headers,
    },
  });

  if (!response.ok) {
    const error = await response.json().catch(() => ({ error: 'Unknown error' }));
    throw new Error(error.error || `HTTP ${response.status}: ${response.statusText}`);
  }

  return response.json();
}

// === Auth ===

export async function signup(request: SignupRequest): Promise<SignupResponse> {
  return fetchJson('/auth/signup', {
    method: 'POST',
    body: JSON.stringify(request),
  });
}

export async function issueChallenge(request: ChallengeRequest): Promise<ChallengeResponse> {
  return fetchJson('/auth/challenge', {
    method: 'POST',
    body: JSON.stringify(request),
  });
}

export async function verifyChallenge(request: VerifyRequest): Promise<VerifyResponse> {
  return fetchJson('/auth/verify', {
    method: 'POST',
    body: JSON.stringify(request),
  });
}

// === Devices ===

export async function addDevice(request: AddDeviceRequest): Promise<AddDeviceResponse> {
  return fetchJson('/me/devices/add', {
    method: 'POST',
    body: JSON.stringify(request),
  });
}

export async function revokeDevice(deviceId: string, request: RevokeDeviceRequest): Promise<void> {
  return fetchJson(`/me/devices/${deviceId}/revoke`, {
    method: 'POST',
    body: JSON.stringify(request),
  });
}

// === Endorsements ===

export async function createEndorsement(
  request: EndorsementCreateRequest
): Promise<EndorsementCreateResponse> {
  return fetchJson('/endorsements', {
    method: 'POST',
    body: JSON.stringify(request),
  });
}

export async function revokeEndorsement(
  endorsementId: string,
  request: EndorsementRevokeRequest
): Promise<void> {
  return fetchJson(`/endorsements/${endorsementId}/revoke`, {
    method: 'POST',
    body: JSON.stringify(request),
  });
}

// === Recovery ===

export async function getRecoveryPolicy(accountId: string): Promise<RecoveryPolicyView | null> {
  try {
    return await fetchJson(`/me/recovery_policy?account_id=${accountId}`);
  } catch (error) {
    if (error instanceof Error && error.message.includes('404')) {
      return null;
    }
    throw error;
  }
}

export async function setRecoveryPolicy(
  request: RecoveryPolicyRequest
): Promise<RecoveryPolicyResponse> {
  return fetchJson('/me/recovery_policy', {
    method: 'POST',
    body: JSON.stringify(request),
  });
}

export async function approveRecovery(
  request: RecoveryApprovalRequest
): Promise<RecoveryApprovalResponse> {
  return fetchJson('/recovery/approve', {
    method: 'POST',
    body: JSON.stringify(request),
  });
}

export async function rotateRoot(request: RootRotationRequest): Promise<RootRotationResponse> {
  return fetchJson('/recovery/rotate_root', {
    method: 'POST',
    body: JSON.stringify(request),
  });
}
