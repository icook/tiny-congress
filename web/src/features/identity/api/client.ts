/**
 * API client for identity endpoints
 * Handles signup, login, device management, and endorsements
 */

const API_BASE_URL = import.meta.env.VITE_API_URL || 'http://localhost:8080';

export class ApiError extends Error {
  constructor(
    message: string,
    public status: number,
    public body?: unknown
  ) {
    super(message);
    this.name = 'ApiError';
  }
}

async function fetchJson<T>(endpoint: string, options: RequestInit = {}): Promise<T> {
  const url = `${API_BASE_URL}${endpoint}`;
  const headers = {
    'Content-Type': 'application/json',
    ...options.headers,
  };

  const response = await fetch(url, {
    ...options,
    headers,
  });

  const body = await response.json().catch(() => ({}));

  if (!response.ok) {
    throw new ApiError(body.error || `HTTP ${response.status}`, response.status, body);
  }

  return body as T;
}

// Signup types and API

export interface SignupRequest {
  username: string;
  root_kid: string;
  root_pubkey: string;
  device_kid: string;
  device_pubkey: string;
  device_metadata: {
    name: string;
    type: string;
    os?: string;
  };
  delegation_envelope: {
    v: number;
    payload_type: string;
    payload: unknown;
    signer: {
      account_id?: string | null;
      device_id?: string | null;
      kid: string;
    };
    sig: string;
  };
}

export interface SignupResponse {
  account_id: string;
  device_id: string;
  username: string;
}

export async function signup(request: SignupRequest): Promise<SignupResponse> {
  return fetchJson<SignupResponse>('/auth/signup', {
    method: 'POST',
    body: JSON.stringify(request),
  });
}

// Challenge/verify types and APIs

export interface ChallengeRequest {
  account_id: string;
  device_id: string;
}

export interface ChallengeResponse {
  challenge_id: string;
  nonce: string;
  expires_at: string;
}

export async function issueChallenge(request: ChallengeRequest): Promise<ChallengeResponse> {
  return fetchJson<ChallengeResponse>('/auth/challenge', {
    method: 'POST',
    body: JSON.stringify(request),
  });
}

export interface VerifyRequest {
  challenge_id: string;
  account_id: string;
  device_id: string;
  signature: string;
}

export interface VerifyResponse {
  session_id: string;
  token: string;
  expires_at: string;
}

export async function verifyChallenge(request: VerifyRequest): Promise<VerifyResponse> {
  return fetchJson<VerifyResponse>('/auth/verify', {
    method: 'POST',
    body: JSON.stringify(request),
  });
}

// Device management types and APIs

export interface Device {
  device_id: string;
  device_kid: string;
  device_metadata: {
    name: string;
    type: string;
    os?: string;
  };
  created_at: string;
  last_seen?: string;
  revoked_at?: string;
}

export async function listDevices(token: string): Promise<Device[]> {
  return fetchJson<Device[]>('/me/devices', {
    headers: { Authorization: `Bearer ${token}` },
  });
}

export interface AddDeviceRequest {
  device_kid: string;
  device_pubkey: string;
  device_metadata: {
    name: string;
    type: string;
    os?: string;
  };
  delegation_envelope: {
    v: number;
    payload_type: string;
    payload: unknown;
    signer: {
      account_id?: string | null;
      device_id?: string | null;
      kid: string;
    };
    sig: string;
  };
}

export async function addDevice(token: string, request: AddDeviceRequest): Promise<Device> {
  return fetchJson<Device>('/me/devices/add', {
    method: 'POST',
    headers: { Authorization: `Bearer ${token}` },
    body: JSON.stringify(request),
  });
}

export interface RevokeDeviceRequest {
  revocation_envelope: {
    v: number;
    payload_type: string;
    payload: unknown;
    signer: {
      account_id?: string | null;
      device_id?: string | null;
      kid: string;
    };
    sig: string;
  };
}

export async function revokeDevice(
  token: string,
  deviceId: string,
  request: RevokeDeviceRequest
): Promise<void> {
  return fetchJson<void>(`/me/devices/${deviceId}/revoke`, {
    method: 'POST',
    headers: { Authorization: `Bearer ${token}` },
    body: JSON.stringify(request),
  });
}
