import react from '@vitejs/plugin-react';
import { defineConfig } from 'vite';
import istanbul from 'vite-plugin-istanbul';
import tsconfigPaths from 'vite-tsconfig-paths';
import { visualizer } from 'rollup-plugin-visualizer';

const truthy = (value) => (value ?? '').toLowerCase() === 'true' || value === '1';
const enablePlaywrightCoverage = truthy(process.env.PLAYWRIGHT_COVERAGE) || truthy(process.env.CI);
const enableBundleAnalyzer = truthy(process.env.ANALYZE);

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
    enableBundleAnalyzer &&
      visualizer({
        filename: './dist/stats.html',
        open: true,
        gzipSize: true,
        brotliSize: true,
      }),
  ].filter(Boolean),
  server: {
    strictPort: true,
    host: process.env.HOST,
  },
  build: {
    sourcemap: enablePlaywrightCoverage,
    rollupOptions: {
      output: {
        manualChunks: {
          'react-vendor': ['react', 'react-dom'],
          'router': ['@tanstack/react-router'],
          'query': ['@tanstack/react-query'],
          'mantine': ['@mantine/core', '@mantine/hooks'],
          'icons': ['@tabler/icons-react'],
        },
      },
    },
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
    coverage: {
      provider: 'v8',
      reporter: ['text', 'json', 'json-summary', 'html'],
      reportsDirectory: './coverage/vitest',
      exclude: [
        'src/wasm/**',
        'src/wasm.d.ts',
        'src/providers/CryptoProvider.tsx',
        'src/features/identity/keys/__tests__/**',
      ],
      thresholds: {
        statements: 70,
        branches: 60,
        functions: 70,
        lines: 70,
      },
    },
  },
});
