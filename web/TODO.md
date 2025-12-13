# Web Stack Improvements TODO

Remaining improvements from stack review. See PR history for context.

## High Priority

- [ ] **Bundle analyzer** - Add `rollup-plugin-visualizer` or `vite-bundle-visualizer` for build analysis
- [ ] **Runtime validation with Zod** - Add schema validation for API responses

## Medium Priority

- [ ] **More Playwright browsers** - Add Firefox and WebKit to `playwright.config.ts` for cross-browser E2E testing
- [ ] **Commit hooks** - Set up Husky with pre-commit linting (`lint-staged`)

## Low Priority

- [ ] **Document Yarn usage** - Add rationale for Yarn 4 over pnpm/npm to docs

## Completed (This PR)

- [x] Fix recharts version pinning (`"2"` → `"^2.15.0"`)
- [x] Add coverage thresholds (vitest coverage config)
- [x] Enable stricter TypeScript options (noUncheckedIndexedAccess, etc.)
- [x] Migrate postcss.config to ESM (.cjs → .mjs)

## Completed (Other PRs)

- [x] Bundle splitting (manualChunks in vite.config) - feature/optimize-bundle-size
- [x] Error boundaries and React coding standards (PR #89)
