import { createTheme } from '@mantine/core';

// Canonical Mantine theme: adjust colors, typography, radius, and component defaults here.
export const mantineTheme = createTheme({
  primaryColor: 'indigo',
  fontFamily: 'Inter, "Segoe UI", system-ui, -apple-system, sans-serif',
  headings: {
    fontFamily: 'Inter, "Segoe UI", system-ui, -apple-system, sans-serif',
    fontWeight: '700',
  },
  defaultRadius: 'md',
  components: {
    Button: {
      defaultProps: {
        radius: 'md',
      },
    },
    Paper: {
      defaultProps: {
        radius: 'md',
        shadow: 'xs',
      },
    },
    NavLink: {
      defaultProps: {
        variant: 'light',
      },
    },
    TextInput: {
      defaultProps: {
        radius: 'md',
      },
    },
  },
});
