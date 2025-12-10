import { afterEach, describe, expect, it, vi } from 'vitest';
import { buildOAuthStartUrl, exchangeOAuthCode, normalizeSession } from './client';
import type { AuthSession } from './types';

describe('auth client', () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('builds an OAuth start URL with redirect and state params', () => {
    const url = buildOAuthStartUrl('github', 'state-123', {
      baseUrl: 'https://api.example.com',
      redirectUri: 'https://app.example.com/login/callback',
    });

    expect(url).toBe(
      'https://api.example.com/auth/oauth/github/start?redirect_uri=https%3A%2F%2Fapp.example.com%2Flogin%2Fcallback&state=state-123'
    );
  });

  it('normalizes session payloads with alternate field names', () => {
    const session = normalizeSession(
      {
        access_token: 'token-abc',
        refresh_token: 'refresh-xyz',
        profile: { id: 'user-1', name: 'Ada Lovelace', avatar_url: 'https://example.com/avatar' },
      },
      'github'
    );

    expect(session).toEqual<AuthSession>({
      token: 'token-abc',
      refreshToken: 'refresh-xyz',
      user: {
        id: 'user-1',
        name: 'Ada Lovelace',
        avatarUrl: 'https://example.com/avatar',
      },
      expiresAt: undefined,
      provider: 'github',
    });
  });

  it('posts the authorization code to the callback endpoint', async () => {
    const fetchMock = vi.fn().mockResolvedValue({
      ok: true,
      json: () =>
        Promise.resolve({
          token: 'session-token',
          user: { id: '123', name: 'Test User', email: 'test@example.com' },
        }),
    });

    vi.stubGlobal('fetch', fetchMock);

    const session = await exchangeOAuthCode({
      provider: 'github',
      code: 'oauth-code',
      state: 'state-xyz',
      redirectUri: 'https://app.example.com/login/callback',
      baseUrl: 'https://api.example.com',
    });

    expect(fetchMock).toHaveBeenCalledWith(
      'https://api.example.com/auth/oauth/github/callback',
      expect.objectContaining({
        method: 'POST',
        credentials: 'include',
        body: JSON.stringify({
          code: 'oauth-code',
          state: 'state-xyz',
          redirectUri: 'https://app.example.com/login/callback',
        }),
      })
    );
    expect(session.token).toBe('session-token');
    expect(session.user.name).toBe('Test User');
  });
});
