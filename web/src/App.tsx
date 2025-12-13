import '@mantine/core/styles.css';

import { MantineProvider } from '@mantine/core';
import { ErrorBoundary } from './components/ErrorBoundary';
import { QueryProvider } from './providers/QueryProvider';
import { Router } from './Router';
import { mantineTheme } from './theme/mantineTheme';

export default function App() {
  return (
    <ErrorBoundary context="Application">
      <QueryProvider>
        <MantineProvider theme={mantineTheme}>
          <Router />
        </MantineProvider>
      </QueryProvider>
    </ErrorBoundary>
  );
}
