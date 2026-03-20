/**
 * Shared HTTP client for REST API calls.
 *
 * Every feature that needs JSON fetch semantics imports from here.
 * Signing (device-key auth) is handled at the feature level because it
 * depends on identity-specific key management.
 */

import { getApiBaseUrl } from '@/config';

interface ApiErrorResponse {
  error?: string;
}

export async function fetchJson<T>(path: string, options?: RequestInit): Promise<T> {
  const url = `${getApiBaseUrl()}${path}`;

  const merged = new Headers({ 'Content-Type': 'application/json' });
  if (options?.headers) {
    new Headers(options.headers).forEach((value, key) => {
      merged.set(key, value);
    });
  }

  let response: Response;
  try {
    response = await fetch(url, { ...options, headers: merged });
  } catch (err) {
    if (err instanceof DOMException && err.name === 'AbortError') {
      throw new Error('The request was cancelled.');
    }
    throw new Error('Unable to connect. Check your internet connection and try again.');
  }

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
