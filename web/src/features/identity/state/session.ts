/**
 * Session state management
 * Stores account_id, device_id, and session token
 */

interface SessionData {
  accountId: string;
  deviceId: string;
  sessionToken: string;
  expiresAt: string;
  username: string;
}

const SESSION_STORAGE_KEY = 'tinycongress:session';

/**
 * Store session data in localStorage
 */
export function saveSession(data: SessionData): void {
  localStorage.setItem(SESSION_STORAGE_KEY, JSON.stringify(data));
}

/**
 * Get current session data
 */
export function getSession(): SessionData | null {
  const stored = localStorage.getItem(SESSION_STORAGE_KEY);
  if (!stored) {
    return null;
  }

  try {
    const data = JSON.parse(stored) as SessionData;
    // Check if session is expired
    if (new Date(data.expiresAt) < new Date()) {
      clearSession();
      return null;
    }
    return data;
  } catch {
    return null;
  }
}

/**
 * Clear session data
 */
export function clearSession(): void {
  localStorage.removeItem(SESSION_STORAGE_KEY);
}

/**
 * Check if user is logged in
 */
export function isLoggedIn(): boolean {
  return getSession() !== null;
}
