import type { AuthSession, AuthUser, OAuthProvider } from './types';

const apiBaseFromEnv = cleanBaseUrl(
  (import.meta.env.VITE_API_BASE_URL as string | undefined | null) ?? '',
);
const redirectPath =
  (import.meta.env.VITE_OAUTH_REDIRECT_PATH as string | undefined | null) ?? '/login/callback';

function cleanBaseUrl(value?: string | null) {
  if (!value) return '';
  return value.endsWith('/') ? value.slice(0, -1) : value;
}

export function getRedirectUri() {
  const origin =
    typeof window !== 'undefined' && window.location?.origin
      ? window.location.origin
      : 'http://localhost:5173';

  return new URL(redirectPath, origin).toString();
}

export function buildOAuthStartUrl(
  provider: OAuthProvider,
  state: string,
  options?: { baseUrl?: string; redirectUri?: string },
) {
  const baseUrl = cleanBaseUrl(options?.baseUrl ?? apiBaseFromEnv);
  const redirectUri = options?.redirectUri ?? getRedirectUri();
  const query = new URLSearchParams({ redirect_uri: redirectUri, state });

  return `${baseUrl}/auth/oauth/${provider}/start?${query.toString()}`;
}

type ExchangeParams = {
  provider: OAuthProvider;
  code: string;
  state?: string;
  redirectUri?: string;
  baseUrl?: string;
};

export async function exchangeOAuthCode(params: ExchangeParams): Promise<AuthSession> {
  const baseUrl = cleanBaseUrl(params.baseUrl ?? apiBaseFromEnv);
  const redirectUri = params.redirectUri ?? getRedirectUri();

  const response = await fetch(`${baseUrl}/auth/oauth/${params.provider}/callback`, {
    method: 'POST',
    credentials: 'include',
    headers: {
      'Content-Type': 'application/json',
    },
    body: JSON.stringify({
      code: params.code,
      state: params.state,
      redirectUri,
    }),
  });

  if (!response.ok) {
    const errorMessage = await safeReadError(response);
    throw new Error(errorMessage ?? 'Failed to complete OAuth handshake');
  }

  const payload = await response.json();
  return normalizeSession(payload, params.provider);
}

export async function revokeSession(session: AuthSession | null, baseUrl?: string) {
  if (!session) return;

  const url = `${cleanBaseUrl(baseUrl ?? apiBaseFromEnv)}/auth/logout`;

  try {
    await fetch(url, {
      method: 'POST',
      credentials: 'include',
      headers: {
        Authorization: `Bearer ${session.token}`,
      },
    });
  } catch (error) {
    console.warn('Failed to revoke session', error);
  }
}

async function safeReadError(response: Response): Promise<string | null> {
  try {
    const text = await response.text();
    return text || null;
  } catch {
    return null;
  }
}

export function normalizeSession(data: any, provider?: OAuthProvider): AuthSession {
  if (!data) throw new Error('Empty authentication payload');

  const token = data.token ?? data.accessToken ?? data.access_token;
  const refreshToken = data.refreshToken ?? data.refresh_token;
  const rawUser: AuthUser = data.user ?? data.profile ?? {};

  const user: AuthUser = {
    id: rawUser.id ?? rawUser.sub ?? '',
    name: rawUser.name ?? rawUser.email ?? '',
    email: rawUser.email,
    avatarUrl: rawUser.avatarUrl ?? rawUser.avatar_url ?? rawUser.picture,
  };

  if (!token || !user.id || !user.name) {
    throw new Error('Authentication response missing required fields');
  }

  return {
    token,
    refreshToken,
    user,
    expiresAt: data.expiresAt ?? data.expires_at,
    provider,
  };
}
