import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render as testingLibraryRender, type RenderOptions } from '@testing-library/react';
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

type WrapperProps = { children: React.ReactNode };

export function render(ui: React.ReactElement, options?: RenderOptions) {
  const testQueryClient = createTestQueryClient();
  const { wrapper: UserWrapper, ...rest } = options ?? {};

  const CombinedWrapper = ({ children }: WrapperProps) => {
    const content = UserWrapper ? <UserWrapper>{children}</UserWrapper> : children;

    return (
      <QueryClientProvider client={testQueryClient}>
        <MantineProvider theme={mantineTheme}>{content}</MantineProvider>
      </QueryClientProvider>
    );
  };

  return testingLibraryRender(ui, { wrapper: CombinedWrapper, ...rest });
}
