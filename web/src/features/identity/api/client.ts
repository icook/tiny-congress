/**
 * Identity API client
 * Type-safe REST client for identity endpoints
 */

// === Base Configuration ===
const API_BASE_URL: string =
  (import.meta.env.VITE_API_URL as string | undefined) ?? 'http://localhost:8080';

// === Types ===

export interface SignupRequest {
  username: string;
  root_pubkey: string; // base64url
}

export interface SignupResponse {
  account_id: string; // UUID
  root_kid: string;
}

interface ApiErrorResponse {
  error?: string;
}

// === API Functions ===

async function fetchJson<T>(path: string, options?: RequestInit): Promise<T> {
  const url = `${API_BASE_URL}${path}`;

  const response = await fetch(url, {
    ...options,
    headers: { 'Content-Type': 'application/json' },
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

  return response.json() as Promise<T>;
}

// === Auth ===

export async function signup(request: SignupRequest): Promise<SignupResponse> {
  return fetchJson('/auth/signup', {
    method: 'POST',
    body: JSON.stringify(request),
  });
}
