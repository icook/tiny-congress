import react from '@vitejs/plugin-react';
import { defineConfig } from 'vite';
import istanbul from 'vite-plugin-istanbul';
import tsconfigPaths from 'vite-tsconfig-paths';

const truthy = (value) => (value ?? '').toLowerCase() === 'true' || value === '1';
const enablePlaywrightCoverage = truthy(process.env.PLAYWRIGHT_COVERAGE) || truthy(process.env.CI);

export default defineConfig({
  plugins: [
    react(),
    tsconfigPaths(),
    enablePlaywrightCoverage &&
      istanbul({
        include: ['src/**/*.ts', 'src/**/*.tsx'],
        extension: ['.ts', '.tsx'],
        cypress: false,
        requireEnv: false,
        forceBuildInstrument: true,
      }),
  ].filter(Boolean),
  server: {
    strictPort: true,
    host: process.env.HOST,
  },
  build: {
    sourcemap: enablePlaywrightCoverage,
  },
  test: {
    globals: true,
    environment: 'jsdom',
    setupFiles: './vitest.setup.mjs',
    include: [
      '**/*.test.ts',
      '**/*.test.tsx',
      '**/*.spec.ts',
      '**/*.spec.tsx',
    ],
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
