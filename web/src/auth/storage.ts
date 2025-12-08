import type { AuthSession, OAuthProvider } from './types';

const SESSION_KEY = 'tinycongress:auth-session';
const OAUTH_STATE_KEY = 'tinycongress:oauth-state';
const OAUTH_STATE_TTL_MS = 5 * 60 * 1000;

type StoredOAuthRequest = {
  provider: OAuthProvider;
  redirectUri: string;
  nextPath?: string;
  createdAt: number;
};

function safeParse<T>(value: string | null): T | null {
  if (!value) {
    return null;
  }

  try {
    return JSON.parse(value) as T;
  } catch {
    return null;
  }
}

export function readStoredSession(): AuthSession | null {
  const stored = safeParse<AuthSession>(localStorage.getItem(SESSION_KEY));
  if (!stored?.token || !stored?.user) {
    return null;
  }
  return stored;
}

export function writeStoredSession(session: AuthSession) {
  localStorage.setItem(SESSION_KEY, JSON.stringify(session));
}

export function clearStoredSession() {
  localStorage.removeItem(SESSION_KEY);
}

function readOAuthStates(): Record<string, StoredOAuthRequest> {
  return safeParse<Record<string, StoredOAuthRequest>>(sessionStorage.getItem(OAUTH_STATE_KEY)) ?? {};
}

function persistOAuthStates(states: Record<string, StoredOAuthRequest>) {
  sessionStorage.setItem(OAUTH_STATE_KEY, JSON.stringify(states));
}

export function rememberOAuthRequest(
  state: string,
  payload: Omit<StoredOAuthRequest, 'createdAt'>,
) {
  const states = readOAuthStates();
  states[state] = { ...payload, createdAt: Date.now() };
  persistOAuthStates(states);
}

export function consumeOAuthRequest(state: string | undefined): StoredOAuthRequest | null {
  if (!state) {
    return null;
  }

  const states = readOAuthStates();
  const record = states[state];
  delete states[state];
  persistOAuthStates(states);

  if (!record) {
    return null;
  }

  const isExpired = Date.now() - record.createdAt > OAUTH_STATE_TTL_MS;
  return isExpired ? null : record;
}
