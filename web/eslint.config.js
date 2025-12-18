import mantine from 'eslint-config-mantine';
import jestDom from 'eslint-plugin-jest-dom';
import playwright from 'eslint-plugin-playwright';
import reactHooks from 'eslint-plugin-react-hooks';
import storybook from 'eslint-plugin-storybook';
import testingLibrary from 'eslint-plugin-testing-library';
import vitest from 'eslint-plugin-vitest';
import globals from 'globals';
import tseslint from 'typescript-eslint';

const vitestRecommended = vitest.configs.recommended;
const testingLibraryReact = testingLibrary.configs['flat/react'];
const jestDomRecommended = jestDom.configs.recommended;
const playwrightRecommended = playwright.configs['flat/recommended'];
const storybookRecommended = storybook.configs['flat/recommended'];

const testFiles = ['src/**/*.{test,spec}.{ts,tsx}', 'test-utils/**/*.{ts,tsx}'];

export default tseslint.config(
  {
    // Ignore generated artifacts so lint time stays predictable and CI noise is avoided.
    ignores: [
      'dist',
      'coverage/**',
      '.nyc_output/**',
      'reports/**',
      'playwright-report/**',
      'test-results/**',
      'storybook-static/**',
      '.yarn/**',
      'node_modules/**',
      'src/api/generated/**', // Generated code from graphql-codegen
      'src/wasm/**', // Generated code from wasm-pack
      'codegen.ts', // GraphQL codegen config
    ],
  },

  // Keep Mantine's baseline React/TS/a11y coverage intact.
  ...mantine,

  {
    // Enable type-aware linting to surface unsafe promise/any misuse during lint, not just tsc.
    files: ['**/*.{ts,tsx}'],
    languageOptions: {
      parserOptions: {
        // Use ESLint-specific tsconfig so config/test/storybook files are included in the program.
        project: './tsconfig.eslint.json',
        tsconfigRootDir: import.meta.dirname,
      },
    },
  },

  {
    // Enforce correct hook usage and dependency tracking across all TS/TSX files.
    files: ['**/*.{ts,tsx}'],
    plugins: {
      'react-hooks': reactHooks,
    },
    rules: {
      'react-hooks/rules-of-hooks': 'error',
      'react-hooks/exhaustive-deps': 'error',
    },
  },

  {
    // Guard against introducing competing styling systems; keep Mantine-first (see docs/style/STYLE_GUIDE.md).
    files: ['src/**/*.{ts,tsx}', 'test-utils/**/*.{ts,tsx}'],
    rules: {
      'no-restricted-imports': [
        'warn',
        {
          paths: [
            {
              name: 'tailwindcss',
              message: 'Use Mantine components and props instead of adding Tailwind (ADR-005).',
            },
            {
              name: 'styled-components',
              message: 'Do not add styled-components; prefer Mantine props per docs/style/STYLE_GUIDE.md.',
            },
            {
              name: '@emotion/react',
              message: 'Avoid Emotion; follow Mantine-first styling (ADR-005).',
            },
            {
              name: '@emotion/styled',
              message: 'Avoid Emotion; follow Mantine-first styling (ADR-005).',
            },
            {
              name: '@mui/material',
              message: 'Do not mix MUI with Mantine; stick to the Mantine-first model.',
            },
          ],
          patterns: [
            {
              group: ['@emotion/*', '@mui/*'],
              message: 'Do not introduce new styling systems; use Mantine primitives and the shared theme.',
            },
          ],
        },
      ],
    },
  },

  {
    // Tighten unit test hygiene (Vitest globals, Testing Library patterns, jest-dom assertions).
    files: testFiles,
    plugins: {
      vitest,
      'testing-library': testingLibrary,
      'jest-dom': jestDom,
    },
    languageOptions: {
      globals: {
        ...(vitestRecommended.languageOptions?.globals ?? {}),
        ...(testingLibraryReact.languageOptions?.globals ?? {}),
        ...(jestDomRecommended.languageOptions?.globals ?? {}),
        ...globals.vitest,
      },
    },
    settings: {
      ...(testingLibraryReact.settings ?? {}),
    },
    rules: {
      ...(vitestRecommended.rules ?? {}),
      ...(testingLibraryReact.rules ?? {}),
      ...(jestDomRecommended.rules ?? {}),
    },
  },

  {
    // Keep Playwright suites aligned with official best practices and failure patterns.
    files: ['tests/e2e/**/*.{ts,tsx}'],
    plugins: {
      playwright,
    },
    languageOptions: {
      globals: {
        ...(playwrightRecommended.languageOptions?.globals ?? {}),
        ...globals.node,
      },
    },
    rules: {
      ...(playwrightRecommended.rules ?? {}),
      'no-restricted-imports': [
        'error',
        {
          paths: [
            {
              name: '@playwright/test',
              message: 'Import { test, expect } from ./fixtures to enable coverage and shared helpers.',
            },
          ],
        },
      ],
    },
  },
  {
    // Allow fixtures to source Playwright primitives.
    files: ['tests/e2e/fixtures.ts'],
    rules: {
      'no-restricted-imports': 'off',
    },
  },

  {
    // Guard stories and Storybook config so CSF/MDX stay consistent with runtime expectations.
    files: ['src/**/*.story.tsx', '.storybook/**/*.{ts,tsx,js}'],
    plugins: {
      storybook,
    },
    settings: {
      ...(storybookRecommended.settings ?? {}),
    },
    rules: {
      ...(storybookRecommended.rules ?? {}),
    },
  },

  {
    // Give tooling scripts and configs the Node globals they need without polluting app code.
    files: [
      '*.config.{js,ts,cjs,mjs}',
      'scripts/**/*.{js,mjs,ts}',
      '.storybook/**/*.{js,ts}',
      'vite.config.mjs',
      'postcss.config.mjs',
      'playwright.config.ts',
    ],
    languageOptions: {
      globals: {
        ...globals.node,
      },
    },
    rules: {
      // Node scripts rely on console output for CLIs; suppress noisy console bans here only.
      'no-console': 'off',
    },
  },

  {
    // Stories intentionally log interactions; keep console available there.
    files: ['**/*.story.tsx'],
    rules: { 'no-console': 'off' },
  }
);
