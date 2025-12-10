import { render as testingLibraryRender } from '@testing-library/react';
import { MantineProvider } from '@mantine/core';
import { AuthProvider } from '../src/auth/AuthProvider';
import { mantineTheme } from '../src/theme/mantineTheme';

export function render(ui: React.ReactNode) {
  return testingLibraryRender(ui, {
    wrapper: ({ children }: { children: React.ReactNode }) => (
      <MantineProvider theme={mantineTheme}>
        <AuthProvider>{children}</AuthProvider>
      </MantineProvider>
    ),
  });
}
