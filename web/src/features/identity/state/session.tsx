/**
 * Session state management
 * Provides authentication context for the application
 */

import { createContext, useContext, useEffect, useState, type ReactNode } from 'react';
import { clearAllKeys } from '../keys';

// === Types ===

export interface Session {
  accountId: string;
  deviceId: string;
  sessionId: string;
  expiresAt: Date;
}

interface SessionContextValue {
  session: Session | null;
  setSession: (session: Session | null) => void;
  isAuthenticated: boolean;
  logout: () => Promise<void>;
}

// === Context ===

const SessionContext = createContext<SessionContextValue | null>(null);

const SESSION_STORAGE_KEY = 'identity:session';

// === Provider ===

interface SessionProviderProps {
  children: ReactNode;
}

export function SessionProvider({ children }: SessionProviderProps) {
  const [session, setSessionState] = useState<Session | null>(() => {
    // Load from localStorage on mount
    try {
      const stored = localStorage.getItem(SESSION_STORAGE_KEY);
      if (stored) {
        const parsed = JSON.parse(stored);
        // Convert expiresAt string back to Date
        parsed.expiresAt = new Date(parsed.expiresAt);

        // Check if session is expired
        if (parsed.expiresAt > new Date()) {
          return parsed;
        }
      }
    } catch {
      // Invalid session data, ignore
    }
    return null;
  });

  const setSession = (newSession: Session | null) => {
    setSessionState(newSession);

    // Persist to localStorage
    if (newSession) {
      localStorage.setItem(SESSION_STORAGE_KEY, JSON.stringify(newSession));
    } else {
      localStorage.removeItem(SESSION_STORAGE_KEY);
    }
  };

  const logout = async () => {
    // Clear session
    setSession(null);

    // Clear stored keys
    await clearAllKeys();
  };

  // Auto-logout on session expiry
  useEffect(() => {
    if (!session) {
      return;
    }

    const timeUntilExpiry = session.expiresAt.getTime() - Date.now();

    if (timeUntilExpiry <= 0) {
      void logout();
      return;
    }

    const timer = setTimeout(() => {
      void logout();
    }, timeUntilExpiry);

    return () => clearTimeout(timer);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [session]);

  const value: SessionContextValue = {
    session,
    setSession,
    isAuthenticated: session !== null && session.expiresAt > new Date(),
    logout,
  };

  return <SessionContext.Provider value={value}>{children}</SessionContext.Provider>;
}

// === Hook ===

export function useSession() {
  const context = useContext(SessionContext);
  if (!context) {
    throw new Error('useSession must be used within SessionProvider');
  }
  return context;
}
