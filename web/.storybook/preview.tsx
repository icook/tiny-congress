import '@mantine/core/styles.css';

import React, { useCallback, useEffect } from 'react';
import { DARK_MODE_EVENT_NAME } from 'storybook-dark-mode';
import { addons } from 'storybook/preview-api';
import { MantineProvider, useMantineColorScheme } from '@mantine/core';
import { theme } from '../src/theme';

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
    (value: boolean) => setColorScheme(value ? 'dark' : 'light'),
    [setColorScheme]
  );

  useEffect(() => {
    const channel = addons.getChannel();
    channel.on(DARK_MODE_EVENT_NAME, handleColorScheme);
    return () => channel.off(DARK_MODE_EVENT_NAME, handleColorScheme);
  }, [handleColorScheme]);

  return children;
}

export const decorators = [
  (renderStory: any) => <ColorSchemeWrapper>{renderStory()}</ColorSchemeWrapper>,
  (renderStory: any) => <MantineProvider theme={theme}>{renderStory()}</MantineProvider>,
];
