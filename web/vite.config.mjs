import react from '@vitejs/plugin-react';
import { defineConfig } from 'vite';
import tsconfigPaths from 'vite-tsconfig-paths';

export default defineConfig({
  plugins: [react(), tsconfigPaths()],
  server: {
    strictPort: true,
    host: process.env.HOST,
  },
  test: {
    globals: true,
    environment: 'jsdom',
    setupFiles: './vitest.setup.mjs',
    exclude: [
      '**/node_modules/**',
      '**/dist/**',
      '**/.idea/**',
      '**/.git/**',
      '**/.cache/**',
      'tests/e2e/**',
    ],
  },
});
