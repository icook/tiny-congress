import boundaries from 'eslint-plugin-boundaries';
import mantine from 'eslint-config-mantine';
import jestDom from 'eslint-plugin-jest-dom';
import playwright from 'eslint-plugin-playwright';
import reactHooks from 'eslint-plugin-react-hooks';
import storybook from 'eslint-plugin-storybook';
import testingLibrary from 'eslint-plugin-testing-library';
import unicorn from 'eslint-plugin-unicorn';
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
    // Ignore generated artifacts and config files.
    // JS/MJS config files are excluded because eslint-config-mantine's TypeScript parser
    // cannot handle them without type information. They're config files with minimal logic.
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
      // JS/MJS config files - excluded due to TypeScript parser incompatibility
      '*.config.js',
      '*.config.cjs',
      '*.config.mjs',
      '.prettierrc.mjs',
      'scripts/**/*.mjs',
      'vitest.setup.mjs',
    ],
  },

  // Keep Mantine's baseline React/TS/a11y coverage intact.
  ...mantine,

  // Layer TypeScript strict presets for enhanced type safety.
  // These add: no-explicit-any, no-unsafe-* family, prefer-nullish-coalescing, etc.
  // Only apply to TypeScript files to avoid parsing issues with JS/MJS config files.
  {
    files: ['**/*.{ts,tsx}'],
    extends: [...tseslint.configs.strictTypeChecked, ...tseslint.configs.stylisticTypeChecked],
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
    // === Enhanced Strictness Rules ===
    // These rules catch real bugs and enforce best practices.
    files: ['src/**/*.{ts,tsx}'],
    plugins: {
      unicorn,
    },
    rules: {
      // --- TypeScript Promise & Type Safety (errors - catch real bugs) ---
      // Catch unhandled promises (common source of silent failures)
      '@typescript-eslint/no-floating-promises': 'error',
      // Prevent async functions in wrong contexts (onClick={async () => ...})
      '@typescript-eslint/no-misused-promises': 'error',
      // Only await actual Promises
      '@typescript-eslint/await-thenable': 'error',
      // Avoid unnecessary type assertions
      '@typescript-eslint/no-unnecessary-type-assertion': 'error',
      // Exhaustive switch statements on union types
      '@typescript-eslint/switch-exhaustiveness-check': 'error',
      // Prefer optional chaining over && chains
      '@typescript-eslint/prefer-optional-chain': 'error',

      // --- Stylistic Preferences (warnings - auto-fixable) ---
      // Prefer nullish coalescing over logical OR for null/undefined
      '@typescript-eslint/prefer-nullish-coalescing': 'warn',

      // --- React Best Practices (errors - prevent rendering bugs) ---
      // Prevent {count && <Component />} rendering "0" when count is 0
      'react/jsx-no-leaked-render': 'error',
      // Enforce [value, setValue] naming pattern for useState
      'react/hook-use-state': 'error',

      // --- Naming Conventions (errors - enforce codebase standards) ---
      // Enforce consistent naming for types and interfaces (PascalCase)
      '@typescript-eslint/naming-convention': [
        'error',
        { selector: 'interface', format: ['PascalCase'] },
        { selector: 'typeAlias', format: ['PascalCase'] },
        { selector: 'enum', format: ['PascalCase'] },
        { selector: 'enumMember', format: ['PascalCase', 'UPPER_CASE'] },
      ],

      // --- Accessibility (errors - stricter than Mantine defaults) ---
      // Autofocus can cause accessibility issues for screen reader users
      'jsx-a11y/no-autofocus': 'error',

      // --- Deprecation Detection (warnings - actionable but not blocking) ---
      // Surface deprecated API usage before it breaks
      '@typescript-eslint/no-deprecated': 'warn',

      // --- Modern JS Best Practices (errors - Unicorn) ---
      // Prefer node: protocol for built-in modules
      'unicorn/prefer-node-protocol': 'error',
      // Avoid useless undefined (return undefined -> return)
      'unicorn/no-useless-undefined': 'error',
      // Always use new with Error
      'unicorn/throw-new-error': 'error',
      // Prefer Array.find over filter()[0]
      'unicorn/prefer-array-find': 'error',
      // Prefer includes over indexOf !== -1
      'unicorn/prefer-includes': 'error',
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
    // Enforce import boundaries between architectural layers (see docs/interfaces/react-coding-standards.md).
    // Hierarchy: pages → features → shared layers (components, api, providers, theme)
    files: ['src/**/*.{ts,tsx}'],
    plugins: {
      boundaries,
    },
    settings: {
      'boundaries/elements': [
        { type: 'pages', pattern: 'src/pages/*' },
        { type: 'features', pattern: 'src/features/*' },
        { type: 'components', pattern: 'src/components/*' },
        { type: 'api', pattern: 'src/api/*' },
        { type: 'providers', pattern: 'src/providers/*' },
        { type: 'theme', pattern: 'src/theme/*' },
      ],
    },
    rules: {
      'boundaries/element-types': [
        'error',
        {
          default: 'disallow',
          rules: [
            { from: 'pages', allow: ['features', 'components', 'api', 'providers', 'theme'] },
            { from: 'features', allow: ['components', 'api', 'providers', 'theme'] },
            { from: 'components', allow: ['api', 'providers', 'theme'] },
            { from: 'api', allow: ['providers', 'theme'] },
            { from: 'providers', allow: ['theme'] },
          ],
        },
      ],
    },
  },

  {
    // Enforce barrel imports: no deep imports into features or pages from outside.
    files: ['src/**/*.{ts,tsx}'],
    ignores: ['src/features/**/*.{ts,tsx}', 'src/pages/**/*.{ts,tsx}'],
    rules: {
      'no-restricted-imports': [
        'error',
        {
          patterns: [
            {
              group: ['@/features/*/*', '@/features/*/*/*'],
              message: 'Import from feature barrel (@/features/X), not internals.',
            },
            {
              group: ['@/pages/*'],
              message: 'Pages are entry points; do not import from them.',
            },
          ],
        },
      ],
    },
  },

  {
    // Within features: enforce sibling barrel imports (../api not ../api/client).
    files: ['src/features/**/*.{ts,tsx}'],
    rules: {
      'no-restricted-imports': [
        'error',
        {
          patterns: [
            {
              group: ['../*/**'],
              message: 'Import from sibling barrel (../api), not internals (../api/client).',
            },
            {
              group: ['@/features/*'],
              message: 'Features cannot import other features. Lift shared code to @/components, @/api, or @/providers.',
            },
          ],
        },
      ],
    },
  },

  {
    // Tighten unit test hygiene (Vitest globals, Testing Library patterns, jest-dom assertions).
    // Relax strict type rules in tests where mocking often requires flexibility.
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
      // Relax strict type rules in tests - mocking often requires any types
      '@typescript-eslint/no-explicit-any': 'off',
      '@typescript-eslint/no-unsafe-assignment': 'off',
      '@typescript-eslint/no-unsafe-member-access': 'off',
      '@typescript-eslint/no-unsafe-call': 'off',
      '@typescript-eslint/no-unsafe-return': 'off',
      '@typescript-eslint/no-unsafe-argument': 'off',
      '@typescript-eslint/no-empty-function': 'off',
      // Allow spreading class instances in tests for creating mock data
      '@typescript-eslint/no-misused-spread': 'off',
    },
  },

  {
    // Keep Playwright suites aligned with official best practices and failure patterns.
    // Relax strict type rules in E2E tests - test fixtures often need flexibility.
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
      // Relax strict type rules in E2E tests
      '@typescript-eslint/no-explicit-any': 'off',
      '@typescript-eslint/no-unsafe-assignment': 'off',
      '@typescript-eslint/no-unsafe-member-access': 'off',
      '@typescript-eslint/no-non-null-assertion': 'off',
      '@typescript-eslint/no-invalid-void-type': 'off',
      '@typescript-eslint/require-await': 'off',
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
    // Relax strict type rules - Storybook decorators often require any types.
    files: ['src/**/*.story.tsx', '.storybook/**/*.{ts,tsx}'],
    plugins: {
      storybook,
    },
    settings: {
      ...(storybookRecommended.settings ?? {}),
    },
    rules: {
      ...(storybookRecommended.rules ?? {}),
      // Relax strict type rules for Storybook - decorators need flexibility
      '@typescript-eslint/no-explicit-any': 'off',
      '@typescript-eslint/no-unsafe-call': 'off',
      '@typescript-eslint/no-unsafe-assignment': 'off',
      '@typescript-eslint/no-unsafe-member-access': 'off',
    },
  },

  {
    // TypeScript config files need Node globals.
    files: [
      '*.config.ts',
      'scripts/**/*.ts',
      '.storybook/**/*.ts',
      'playwright.config.ts',
    ],
    languageOptions: {
      globals: {
        ...globals.node,
      },
    },
    rules: {
      'no-console': 'off',
    },
  },

  {
    // Stories intentionally log interactions; keep console available there.
    files: ['**/*.story.tsx'],
    rules: { 'no-console': 'off' },
  }
);
