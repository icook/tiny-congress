/**
 * Device context provider — persists device credentials in IndexedDB.
 *
 * On mount, reads credentials from IndexedDB. On setDevice/clearDevice,
 * writes/deletes from IndexedDB and updates React state.
 */

import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from 'react';
import { openDB, type IDBPDatabase } from 'idb';

const DB_NAME = 'tc-device-store';
const DB_VERSION = 1;
const STORE_NAME = 'device';
const CURRENT_KEY = 'current';

interface StoredDevice {
  kid: string;
  privateKey: Uint8Array;
}

interface DeviceContextValue {
  /** Current device KID, or null if not authenticated */
  deviceKid: string | null;
  /** Current device signing key, or null if not authenticated */
  privateKey: Uint8Array | null;
  /** True while loading credentials from IndexedDB on mount */
  isLoading: boolean;
  /** Store device credentials after signup/login */
  setDevice: (kid: string, key: Uint8Array) => void;
  /** Clear device credentials (logout) */
  clearDevice: () => void;
}

// eslint-disable-next-line @typescript-eslint/no-empty-function -- context defaults are never called
const noop = () => {};

const DeviceContext = createContext<DeviceContextValue>({
  deviceKid: null,
  privateKey: null,
  isLoading: true,
  setDevice: noop,
  clearDevice: noop,
});

async function getDb(): Promise<IDBPDatabase> {
  return openDB(DB_NAME, DB_VERSION, {
    upgrade(db) {
      if (!db.objectStoreNames.contains(STORE_NAME)) {
        db.createObjectStore(STORE_NAME);
      }
    },
  });
}

async function loadDevice(): Promise<StoredDevice | undefined> {
  const db = await getDb();
  return db.get(STORE_NAME, CURRENT_KEY) as Promise<StoredDevice | undefined>;
}

async function saveDevice(device: StoredDevice): Promise<void> {
  const db = await getDb();
  await db.put(STORE_NAME, device, CURRENT_KEY);
}

async function deleteDevice(): Promise<void> {
  const db = await getDb();
  await db.delete(STORE_NAME, CURRENT_KEY);
}

interface DeviceProviderProps {
  children: ReactNode;
}

export function DeviceProvider({ children }: DeviceProviderProps) {
  const [deviceKid, setDeviceKid] = useState<string | null>(null);
  const [privateKey, setPrivateKey] = useState<Uint8Array | null>(null);
  const [isLoading, setIsLoading] = useState(true);

  // Load from IndexedDB on mount
  useEffect(() => {
    loadDevice()
      .then((stored) => {
        if (stored) {
          setDeviceKid(stored.kid);
          setPrivateKey(stored.privateKey);
        }
      })
      .catch((err: unknown) => {
        // IndexedDB may be unavailable (incognito, etc.) — degrade gracefully
        // eslint-disable-next-line no-console
        console.warn('Failed to load device from IndexedDB:', err);
      })
      .finally(() => {
        setIsLoading(false);
      });
  }, []);

  const setDeviceFn = useCallback((kid: string, key: Uint8Array) => {
    setDeviceKid(kid);
    setPrivateKey(key);
    saveDevice({ kid, privateKey: key }).catch((err: unknown) => {
      // eslint-disable-next-line no-console
      console.warn('Failed to save device to IndexedDB:', err);
    });
  }, []);

  const clearDeviceFn = useCallback(() => {
    setDeviceKid(null);
    setPrivateKey(null);
    deleteDevice().catch((err: unknown) => {
      // eslint-disable-next-line no-console
      console.warn('Failed to delete device from IndexedDB:', err);
    });
  }, []);

  const value = useMemo(
    () => ({
      deviceKid,
      privateKey,
      isLoading,
      setDevice: setDeviceFn,
      clearDevice: clearDeviceFn,
    }),
    [deviceKid, privateKey, isLoading, setDeviceFn, clearDeviceFn]
  );

  return <DeviceContext.Provider value={value}>{children}</DeviceContext.Provider>;
}

/**
 * Hook to access device credentials.
 */
export function useDevice(): DeviceContextValue {
  return useContext(DeviceContext);
}
