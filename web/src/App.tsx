import '@mantine/core/styles.css';
import '@mantine/notifications/styles.css';

import { MantineProvider } from '@mantine/core';
import { Notifications } from '@mantine/notifications';
import { ErrorBoundary } from './components/ErrorBoundary';
import { CryptoProvider } from './providers/CryptoProvider';
import { DeviceProvider } from './providers/DeviceProvider';
import { QueryProvider } from './providers/QueryProvider';
import { Router } from './Router';
import { mantineTheme } from './theme/mantineTheme';

export default function App() {
  return (
    <ErrorBoundary context="Application">
      <CryptoProvider>
        <DeviceProvider>
          <QueryProvider>
            <MantineProvider theme={mantineTheme} defaultColorScheme="dark">
              <Notifications />
              <Router />
            </MantineProvider>
          </QueryProvider>
        </DeviceProvider>
      </CryptoProvider>
    </ErrorBoundary>
  );
}
