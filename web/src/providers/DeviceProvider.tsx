/**
 * Device context provider — session-only storage of current device credentials.
 *
 * After signup, the device KID and signing key are stored here so that
 * authenticated API calls can be made. This is session-only (lost on refresh).
 * IndexedDB persistence is planned for M3.
 */

import { createContext, useCallback, useContext, useMemo, useState, type ReactNode } from 'react';

interface DeviceContextValue {
  /** Current device KID, or null if not authenticated */
  deviceKid: string | null;
  /** Current device signing key, or null if not authenticated */
  privateKey: Uint8Array | null;
  /** Store device credentials after signup */
  setDevice: (kid: string, key: Uint8Array) => void;
  /** Clear device credentials (logout) */
  clearDevice: () => void;
}

// eslint-disable-next-line @typescript-eslint/no-empty-function -- context defaults are never called
const noop = () => {};

const DeviceContext = createContext<DeviceContextValue>({
  deviceKid: null,
  privateKey: null,
  setDevice: noop,
  clearDevice: noop,
});

interface DeviceProviderProps {
  children: ReactNode;
}

export function DeviceProvider({ children }: DeviceProviderProps) {
  const [deviceKid, setDeviceKid] = useState<string | null>(null);
  const [privateKey, setPrivateKey] = useState<Uint8Array | null>(null);

  const setDevice = useCallback((kid: string, key: Uint8Array) => {
    setDeviceKid(kid);
    setPrivateKey(key);
  }, []);

  const clearDevice = useCallback(() => {
    setDeviceKid(null);
    setPrivateKey(null);
  }, []);

  const value = useMemo(
    () => ({ deviceKid, privateKey, setDevice, clearDevice }),
    [deviceKid, privateKey, setDevice, clearDevice]
  );

  return <DeviceContext.Provider value={value}>{children}</DeviceContext.Provider>;
}

/**
 * Hook to access device credentials.
 */
export function useDevice(): DeviceContextValue {
  return useContext(DeviceContext);
}
