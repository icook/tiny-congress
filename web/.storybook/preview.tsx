import '@mantine/core/styles.css';

import React, { useCallback, useEffect } from 'react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { DARK_MODE_EVENT_NAME } from '@vueless/storybook-dark-mode';
import { addons } from 'storybook/preview-api';
import { MantineProvider, useMantineColorScheme } from '@mantine/core';
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

export const decorators = [
  (renderStory: any) => <ColorSchemeWrapper>{renderStory()}</ColorSchemeWrapper>,
  (renderStory: any) => <MantineProvider theme={mantineTheme}>{renderStory()}</MantineProvider>,
  (renderStory: any) => (
    <QueryClientProvider client={queryClient}>{renderStory()}</QueryClientProvider>
  ),
];
