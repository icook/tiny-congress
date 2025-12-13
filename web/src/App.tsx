import '@mantine/core/styles.css';

import { MantineProvider } from '@mantine/core';
import { ErrorBoundary } from './components/ErrorBoundary';
import { SessionProvider } from './features/identity/state/session';
import { QueryProvider } from './providers/QueryProvider';
import { Router } from './Router';
import { mantineTheme } from './theme/mantineTheme';

export default function App() {
  return (
    <ErrorBoundary context="Application">
      <QueryProvider>
        <MantineProvider theme={mantineTheme}>
          <SessionProvider>
            <Router />
          </SessionProvider>
        </MantineProvider>
      </QueryProvider>
    </ErrorBoundary>
  );
}
