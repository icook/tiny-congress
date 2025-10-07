# Tiny Congress Web

## Features

This template comes with the following features:

- [PostCSS](https://postcss.org/) with [mantine-postcss-preset](https://mantine.dev/styles/postcss-preset)
- [TypeScript](https://www.typescriptlang.org/)
- [Storybook](https://storybook.js.org/)
- [Vitest](https://vitest.dev/) setup with [React Testing Library](https://testing-library.com/docs/react-testing-library/intro)
- ESLint setup with [eslint-config-mantine](https://github.com/mantinedev/eslint-config-mantine)

## npm scripts

## Build and dev scripts

- `dev` – start development server
- `build` – build production version of the app
- `preview` – locally preview production build

### Testing scripts

- `typecheck` – checks TypeScript types
- `lint` – runs ESLint
- `prettier` – checks files with Prettier
- `prettier:write` – formats all files with Prettier
- `vitest` – runs unit tests
- `vitest:watch` – starts vitest watch
- `test` – runs `typecheck`, `prettier`, `lint`, `vitest` and `build`
- `playwright:test` – executes Chromium end-to-end tests (without code coverage instrumentation)
- `playwright:report` – merges any `.nyc_output` artifacts into `coverage/playwright/lcov.info`
- `playwright:clean` – removes previous Playwright junit, coverage, and trace artifacts
- `playwright:ci` – convenience wrapper that cleans artifacts, runs `playwright:test` with coverage, then calls `playwright:report`

### Other scripts

- `storybook` – starts storybook dev server
- `storybook:build` – build production storybook bundle to `storybook-static`

## Playwright coverage workflow

CI publishes Playwright results to GitHub's Tests and coverage dashboards using the generated
`reports/playwright.xml` (JUnit) and `coverage/playwright/lcov.info` (LCOV). To reproduce locally:

1. `cd web`
2. Run `yarn playwright:ci`
3. Inspect the `coverage/playwright/lcov.info` file directly or emit a textual summary with
   `yarn playwright:report --reporter=text-summary`

During CI (or whenever `PLAYWRIGHT_COVERAGE=true`), Vite instruments compiled assets and the E2E
fixtures persist per-test coverage under `.nyc_output/`. `nyc report` converts that data into
`coverage/playwright/lcov.info`, which GitHub Actions uploads to the repository coverage dashboard.
JUnit metadata lives under `reports/`, while any failure artifacts (traces/videos) land in
`test-results/`.
