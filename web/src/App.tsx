import '@mantine/core/styles.css';
import '@mantine/notifications/styles.css';

import { MantineProvider } from '@mantine/core';
import { Notifications } from '@mantine/notifications';
import { BrowserCapabilityGate } from './components/BrowserCapabilityGate';
import { ErrorBoundary } from './components/ErrorBoundary';
import { CryptoProvider } from './providers/CryptoProvider';
import { DeviceProvider } from './providers/DeviceProvider';
import { QueryProvider } from './providers/QueryProvider';
import { Router } from './Router';
import { mantineTheme } from './theme/mantineTheme';

export default function App() {
  return (
    <ErrorBoundary context="Application">
      <MantineProvider theme={mantineTheme} defaultColorScheme="dark">
        <Notifications />
        <BrowserCapabilityGate>
          <CryptoProvider>
            <DeviceProvider>
              <QueryProvider>
                <Router />
              </QueryProvider>
            </DeviceProvider>
          </CryptoProvider>
        </BrowserCapabilityGate>
      </MantineProvider>
    </ErrorBoundary>
  );
}
