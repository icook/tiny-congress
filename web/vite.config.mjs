import react from '@vitejs/plugin-react';
import { defineConfig } from 'vite';
import tsconfigPaths from 'vite-tsconfig-paths';
import { visualizer } from 'rollup-plugin-visualizer';

const truthy = (value) => (value ?? '').toLowerCase() === 'true' || value === '1';
const enablePlaywrightCoverage = truthy(process.env.PLAYWRIGHT_COVERAGE) || truthy(process.env.CI);
const enableBundleAnalyzer = truthy(process.env.ANALYZE);

export default defineConfig({
  plugins: [
    react(),
    tsconfigPaths(),
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
      thresholds: {
        statements: 50,
        branches: 50,
        functions: 50,
        lines: 50,
      },
    },
  },
});
