import { createContext, ReactNode, useCallback, useContext, useMemo, useState } from 'react';
import { buildOAuthStartUrl, exchangeOAuthCode, getRedirectUri, revokeSession } from './client';
import {
  clearStoredSession,
  consumeOAuthRequest,
  readStoredSession,
  rememberOAuthRequest,
  writeStoredSession,
} from './storage';
import type { AuthSession, AuthStatus, AuthUser, OAuthProvider } from './types';

type CompleteOAuthParams = {
  code: string;
  state?: string;
  provider?: OAuthProvider;
};

type CompleteOAuthResult = {
  nextPath?: string;
  session?: AuthSession;
};

type AuthContextValue = {
  status: AuthStatus;
  session: AuthSession | null;
  user: AuthUser | null;
  error: string | null;
  loginWithProvider: (provider: OAuthProvider, nextPath?: string) => void;
  completeOAuth: (params: CompleteOAuthParams) => Promise<CompleteOAuthResult>;
  logout: () => Promise<void>;
};

const AuthContext = createContext<AuthContextValue | undefined>(undefined);

export function AuthProvider({ children }: { children: ReactNode }) {
  const [session, setSession] = useState<AuthSession | null>(() => readStoredSession());
  const [status, setStatus] = useState<AuthStatus>(
    session ? 'authenticated' : 'unauthenticated',
  );
  const [error, setError] = useState<string | null>(null);

  const loginWithProvider = useCallback(
    (provider: OAuthProvider, nextPath?: string) => {
      setError(null);
      setStatus('authenticating');

      const state = crypto.randomUUID?.() ?? Math.random().toString(36).slice(2);
      rememberOAuthRequest(state, { provider, redirectUri: getRedirectUri(), nextPath });

      const startUrl = buildOAuthStartUrl(provider, state);
      window.location.assign(startUrl);
    },
    [],
  );

  const completeOAuth = useCallback(
    async ({ code, state, provider }: CompleteOAuthParams): Promise<CompleteOAuthResult> => {
      const pending = consumeOAuthRequest(state);
      const redirectUri = pending?.redirectUri ?? getRedirectUri();
      const nextPath = pending?.nextPath ?? '/dashboard';
      const selectedProvider = provider ?? pending?.provider;

      if (!selectedProvider) {
        const missingProviderError = 'Missing OAuth provider; restart the sign-in flow.';
        setError(missingProviderError);
        setStatus('error');
        throw new Error(missingProviderError);
      }

      setError(null);
      setStatus('authenticating');

      try {
        const freshSession = await exchangeOAuthCode({
          provider: selectedProvider,
          code,
          state,
          redirectUri,
        });

        writeStoredSession(freshSession);
        setSession(freshSession);
        setStatus('authenticated');

        return { session: freshSession, nextPath };
      } catch (err) {
        const message = err instanceof Error ? err.message : 'Login failed';
        setError(message);
        setStatus('error');
        throw err;
      }
    },
    [],
  );

  const logout = useCallback(async () => {
    setStatus('authenticating');
    setError(null);

    await revokeSession(session);
    clearStoredSession();
    setSession(null);
    setStatus('unauthenticated');
  }, [session]);

  const value = useMemo<AuthContextValue>(
    () => ({
      status,
      session,
      user: session?.user ?? null,
      error,
      loginWithProvider,
      completeOAuth,
      logout,
    }),
    [status, session, error, loginWithProvider, completeOAuth, logout],
  );

  return <AuthContext.Provider value={value}>{children}</AuthContext.Provider>;
}

export function useAuth() {
  const context = useContext(AuthContext);
  if (!context) {
    throw new Error('useAuth must be used within AuthProvider');
  }
  return context;
}
