export type OAuthProvider = 'github' | 'google';

export type AuthStatus = 'unauthenticated' | 'authenticating' | 'authenticated' | 'error';

export interface AuthUser {
  id: string;
  name: string;
  email?: string;
  avatarUrl?: string;
}

export interface AuthSession {
  token: string;
  refreshToken?: string;
  user: AuthUser;
  expiresAt?: string;
  provider?: OAuthProvider;
}
