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

/**
 * Callback invoked when any non-auth endpoint returns 401.
 * Registered by DeviceProvider to clear credentials and redirect to login.
 */
let on401Handler: (() => void) | null = null;

/**
 * Register a handler to be called on unexpected 401 responses.
 * Replaces any previously registered handler.
 */
export function setOn401Handler(handler: () => void): void {
  on401Handler = handler;
}

/**
 * Remove the registered 401 handler (e.g. on unmount in tests).
 */
export function clearOn401Handler(): void {
  on401Handler = null;
}

/**
 * Paths that are expected to return 401 during normal usage (unauthenticated
 * requests). We must not redirect on these or we create redirect loops.
 */
const AUTH_PATHS = ['/auth/login', '/auth/signup', '/auth/backup/'];

function isAuthPath(path: string): boolean {
  return AUTH_PATHS.some((prefix) => path.startsWith(prefix));
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

  if (response.status === 401 && !isAuthPath(path)) {
    on401Handler?.();
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
