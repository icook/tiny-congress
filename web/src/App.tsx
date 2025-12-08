import '@mantine/core/styles.css';

import { MantineProvider } from '@mantine/core';
import { Router } from './Router';
import { AuthProvider } from './auth/AuthProvider';
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
