import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
  clearStoredSession,
  consumeOAuthRequest,
  readStoredSession,
  rememberOAuthRequest,
  writeStoredSession,
} from './storage';
import type { AuthSession } from './types';

const session: AuthSession = {
  token: 'token-123',
  refreshToken: 'refresh-123',
  user: { id: 'user-1', name: 'Test User' },
  expiresAt: '2030-01-01T00:00:00Z',
  provider: 'github',
};

describe('auth storage', () => {
  beforeEach(() => {
    localStorage.clear();
    sessionStorage.clear();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('persists and restores an auth session', () => {
    writeStoredSession(session);
    expect(readStoredSession()).toEqual(session);

    clearStoredSession();
    expect(readStoredSession()).toBeNull();
  });

  it('stores and consumes OAuth request metadata', () => {
    rememberOAuthRequest('state-123', {
      provider: 'github',
      redirectUri: 'http://localhost/login/callback',
      nextPath: '/dashboard',
    });

    const consumed = consumeOAuthRequest('state-123');
    expect(consumed).toMatchObject({
      provider: 'github',
      redirectUri: 'http://localhost/login/callback',
      nextPath: '/dashboard',
    });

    // Subsequent reads should be cleared
    expect(consumeOAuthRequest('state-123')).toBeNull();
  });

  it('expires old OAuth state payloads', () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2024-01-01T00:00:00Z'));

    rememberOAuthRequest('state-abc', {
      provider: 'google',
      redirectUri: 'http://localhost/login/callback',
    });

    vi.setSystemTime(new Date('2024-01-01T00:07:00Z'));

    expect(consumeOAuthRequest('state-abc')).toBeNull();
  });
});
