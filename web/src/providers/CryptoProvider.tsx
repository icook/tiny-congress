import { createContext, useContext, useEffect, useState, type ReactNode } from 'react';

/**
 * Interface for the crypto module functions exposed by WASM
 */
export interface CryptoModule {
  /** Derive a Key ID from a public key: base64url(SHA-256(pubkey)[0:16]) */
  derive_kid: (publicKey: Uint8Array) => string;
  /** Encode bytes as base64url (RFC 4648) without padding */
  encode_base64url: (bytes: Uint8Array) => string;
  /** Decode a base64url string to bytes */
  decode_base64url: (encoded: string) => Uint8Array;
}

interface CryptoContextValue {
  /** The loaded crypto module, or null if still loading */
  crypto: CryptoModule | null;
  /** Whether the WASM module is currently loading */
  isLoading: boolean;
  /** Error if WASM failed to load */
  error: Error | null;
}

const CryptoContext = createContext<CryptoContextValue>({
  crypto: null,
  isLoading: true,
  error: null,
});

interface CryptoProviderProps {
  children: ReactNode;
}

/**
 * Provider that loads the tc-crypto WASM module and makes it available
 * to child components via the useCrypto hook.
 *
 * The WASM module is loaded asynchronously on mount. Children will not
 * render until the module is loaded (renders null during loading).
 */
export function CryptoProvider({ children }: CryptoProviderProps) {
  const [state, setState] = useState<CryptoContextValue>({
    crypto: null,
    isLoading: true,
    error: null,
  });

  useEffect(() => {
    let mounted = true;

    async function loadWasm() {
      try {
        // Dynamic import for code splitting - WASM loads separately from main bundle
        const wasm = await import('@/wasm/tc-crypto/tc_crypto.js');
        // Initialize the WASM module (required before using exported functions)
        await wasm.default();

        if (mounted) {
          setState({
            crypto: {
              derive_kid: wasm.derive_kid,
              encode_base64url: wasm.encode_base64url,
              decode_base64url: wasm.decode_base64url,
            },
            isLoading: false,
            error: null,
          });
        }
      } catch (err) {
        if (mounted) {
          setState({
            crypto: null,
            isLoading: false,
            error: err instanceof Error ? err : new Error('Failed to load crypto WASM module'),
          });
        }
      }
    }

    loadWasm();
    return () => {
      mounted = false;
    };
  }, []);

  // Don't render children until WASM is loaded
  // This ensures crypto is always available when useCryptoRequired is called
  if (state.isLoading) {
    return null;
  }

  if (state.error) {
    // Let ErrorBoundary handle it or show inline error
    throw state.error;
  }

  return <CryptoContext.Provider value={state}>{children}</CryptoContext.Provider>;
}

/**
 * Hook to access the crypto module state.
 * Returns { crypto, isLoading, error } - check isLoading/error before using crypto.
 */
export function useCrypto(): CryptoContextValue {
  return useContext(CryptoContext);
}

/**
 * Hook to access the crypto module directly.
 * Throws if the crypto module is not loaded yet.
 * Only use this inside components that are descendants of CryptoProvider.
 */
export function useCryptoRequired(): CryptoModule {
  const { crypto, isLoading, error } = useContext(CryptoContext);

  if (isLoading) {
    throw new Error(
      'Crypto module is still loading. Ensure this component is inside CryptoProvider.'
    );
  }
  if (error) {
    throw error;
  }
  if (!crypto) {
    throw new Error('Crypto module not available. Ensure this component is inside CryptoProvider.');
  }

  return crypto;
}
