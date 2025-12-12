import '@mantine/core/styles.css';

import { MantineProvider } from '@mantine/core';
import { QueryProvider } from './providers/QueryProvider';
import { Router } from './Router';
import { mantineTheme } from './theme/mantineTheme';

export default function App() {
  return (
    <QueryProvider>
      <MantineProvider theme={mantineTheme}>
        <Router />
      </MantineProvider>
    </QueryProvider>
  );
}
