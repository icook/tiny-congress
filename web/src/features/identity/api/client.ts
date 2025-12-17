/**
 * Identity API client
 * Type-safe REST client for identity endpoints
 */

// === Base Configuration ===
const API_BASE_URL = import.meta.env.VITE_API_URL || 'http://localhost:8080';

// === Types ===

export interface SignupRequest {
  username: string;
  root_pubkey: string; // base64url
}

export interface SignupResponse {
  account_id: string; // UUID
  root_kid: string;
}

// === API Functions ===

async function fetchJson<T>(path: string, options?: RequestInit): Promise<T> {
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
