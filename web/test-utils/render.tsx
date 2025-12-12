import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render as testingLibraryRender } from '@testing-library/react';
import { MantineProvider } from '@mantine/core';
import { mantineTheme } from '../src/theme/mantineTheme';

function createTestQueryClient() {
  return new QueryClient({
    defaultOptions: {
      queries: {
        retry: false, // Don't retry in tests
        gcTime: Infinity, // Keep cache for test duration
      },
    },
  });
}

export function render(ui: React.ReactNode) {
  const testQueryClient = createTestQueryClient();

  return testingLibraryRender(ui, {
    wrapper: ({ children }: { children: React.ReactNode }) => (
      <QueryClientProvider client={testQueryClient}>
        <MantineProvider theme={mantineTheme}>{children}</MantineProvider>
      </QueryClientProvider>
    ),
  });
}
