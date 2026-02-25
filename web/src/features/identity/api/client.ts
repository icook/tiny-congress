/**
 * Identity API client
 * Type-safe REST client for identity endpoints
 */

import { getApiBaseUrl } from '@/config';

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

  return response.json() as Promise<T>;
}

// === Auth ===

export async function signup(request: SignupRequest): Promise<SignupResponse> {
  return fetchJson('/auth/signup', {
    method: 'POST',
    body: JSON.stringify(request),
  });
}
