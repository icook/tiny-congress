import '@mantine/core/styles.css';

import React, { useCallback, useEffect, useRef, useState } from 'react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import {
  createMemoryHistory,
  createRootRouteWithContext,
  createRouter,
  RouterProvider,
} from '@tanstack/react-router';
import { DARK_MODE_EVENT_NAME } from '@vueless/storybook-dark-mode';
import { addons } from 'storybook/preview-api';
import { MantineProvider, useMantineColorScheme } from '@mantine/core';
import { CryptoContext, type CryptoModule } from '../src/providers/CryptoProvider';
import { mantineTheme } from '../src/theme/mantineTheme';

export const parameters = {
  layout: 'fullscreen',
  options: {
    showPanel: false,
  },
};

function ColorSchemeWrapper({ children }: { children: React.ReactNode }) {
  const { setColorScheme } = useMantineColorScheme();
  // useCallback keeps the handler stable so the dark-mode listener doesn't re-register on every render.
  const handleColorScheme = useCallback(
    (value: boolean) => {
      setColorScheme(value ? 'dark' : 'light');
    },
    [setColorScheme]
  );

  useEffect(() => {
    const channel = addons.getChannel();
    channel.on(DARK_MODE_EVENT_NAME, handleColorScheme);
    return () => {
      channel.off(DARK_MODE_EVENT_NAME, handleColorScheme);
    };
  }, [handleColorScheme]);

  return children;
}

const queryClient = new QueryClient({
  defaultOptions: { queries: { retry: false } },
});

// Stub crypto module for components that call useCrypto/useCryptoRequired.
// WASM is not available in Storybook (no Rust toolchain in CI).
const stubCrypto: CryptoModule = {
  derive_kid: () => 'AAAAAAAAAAAAAAAAAAAAAA',
  encode_base64url: () => '',
  decode_base64url: () => new Uint8Array(),
};

// Wrapper that provides TanStack Router context so Link/useNavigate don't throw.
// Creates a single-route router whose root component renders children via a ref
// so it always shows the latest story content without recreating the router.
function StorybookRouter({ children }: { children: React.ReactNode }) {
  const childrenRef = useRef<React.ReactNode>(children);
  childrenRef.current = children;

  const [router] = useState(() => {
    const root = createRootRouteWithContext<{ auth: { deviceKid: string | null } }>()({
      component: () => <>{childrenRef.current}</>,
    });
    return createRouter({
      routeTree: root,
      history: createMemoryHistory({ initialEntries: ['/'] }),
      context: { auth: { deviceKid: null } },
    });
  });

  return <RouterProvider router={router} />;
}

export const decorators = [
  (renderStory: any) => <ColorSchemeWrapper>{renderStory()}</ColorSchemeWrapper>,
  (renderStory: any) => <MantineProvider theme={mantineTheme}>{renderStory()}</MantineProvider>,
  (renderStory: any) => (
    <QueryClientProvider client={queryClient}>{renderStory()}</QueryClientProvider>
  ),
  (renderStory: any) => (
    <CryptoContext.Provider value={{ crypto: stubCrypto, isLoading: false, error: null }}>
      {renderStory()}
    </CryptoContext.Provider>
  ),
  (renderStory: any) => <StorybookRouter>{renderStory()}</StorybookRouter>,
];
