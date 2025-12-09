import '@mantine/core/styles.css';

import { MantineProvider } from '@mantine/core';
import { AuthProvider } from './auth/AuthProvider';
import { Router } from './Router';
import { mantineTheme } from './theme/mantineTheme';

export default function App() {
  return (
    <MantineProvider theme={mantineTheme}>
      <AuthProvider>
        <Router />
      </AuthProvider>
    </MantineProvider>
  );
}
