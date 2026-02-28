# Source-Generated Docs Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Publish Rust and TypeScript API documentation to Cloudflare Pages with PR previews.

**Architecture:** Replace gh-pages branch deployment with Cloudflare Pages. Add cargo doc and TypeDoc generation as a new CI job. Merge all artifacts (coverage, storybook, playwright, docs) into a single site deployed to CF.

**Tech Stack:** Cloudflare Pages, cargo doc, TypeDoc, GitHub Actions

---

## Task 1: Add TypeDoc Dependency

**Files:**
- Modify: `web/package.json`

**Step 1: Add typedoc as dev dependency**

```bash
cd web && yarn add -D typedoc
```

**Step 2: Verify installation**

Run: `cd web && yarn typedoc --version`
Expected: Version number (e.g., `0.27.x`)

**Step 3: Commit**

```bash
git add web/package.json web/yarn.lock
git commit -m "chore(web): add typedoc dependency"
```

---

## Task 2: Create TypeDoc Configuration

**Files:**
- Create: `web/typedoc.json`
- Modify: `web/.gitignore`

**Step 1: Create typedoc.json**

Create file `web/typedoc.json`:

```json
{
  "$schema": "https://typedoc.org/schema.json",
  "entryPoints": ["src"],
  "entryPointStrategy": "expand",
  "out": "docs",
  "exclude": [
    "**/*.test.ts",
    "**/*.test.tsx",
    "**/__mocks__/**",
    "**/test-utils/**",
    "src/wasm/**"
  ],
  "excludePrivate": true,
  "excludeInternal": true,
  "readme": "none",
  "name": "Tiny Congress Frontend"
}
```

**Step 2: Add docs/ to .gitignore**

Add to `web/.gitignore`:

```
# TypeDoc output
docs/
```

**Step 3: Test TypeDoc locally**

Run: `cd web && yarn typedoc`
Expected: `Documentation generated at ./docs`

Run: `ls web/docs/`
Expected: `index.html` and other generated files

**Step 4: Commit**

```bash
git add web/typedoc.json web/.gitignore
git commit -m "chore(web): add typedoc configuration"
```

---

## Task 3: Add Justfile Documentation Recipes

**Files:**
- Modify: `justfile`

**Step 1: Add docs recipes to justfile**

Add after the "Utility Commands" section, before "Git Workflows":

```just
# =============================================================================
# Documentation
# =============================================================================

# Build all documentation locally
docs: docs-rust docs-ts
    @echo "✓ All docs built"
    @echo "  Rust: target/doc/tinycongress_api/index.html"
    @echo "  TypeScript: web/docs/index.html"

# Build and open Rust API docs
docs-rust:
    cargo doc --workspace --no-deps --open

# Build TypeScript docs
docs-ts:
    cd web && yarn typedoc
    @echo "TypeScript docs: web/docs/index.html"
```

**Step 2: Test recipes**

Run: `just docs-rust`
Expected: Browser opens with Rust documentation

Run: `just docs-ts`
Expected: `TypeScript docs: web/docs/index.html`

**Step 3: Commit**

```bash
git add justfile
git commit -m "chore: add documentation build recipes to justfile"
```

---

## Task 4: Add build-docs CI Job

**Files:**
- Modify: `.github/workflows/ci.yml`

**Step 1: Add build-docs job**

Add after the `storybook-build` job (around line 106):

```yaml
  # ============================================================
  # JOB 0b3: Build API Documentation
  # ============================================================
  build-docs:
    name: Build documentation
    runs-on: ubuntu-latest
    steps:
      - name: Checkout
        uses: actions/checkout@v6

      - name: Set up Rust
        uses: dtolnay/rust-toolchain@stable

      - name: Cache Rust dependencies
        uses: Swatinem/rust-cache@v2

      - name: Build Rust docs
        run: cargo doc --workspace --no-deps

      - name: Set up Node
        uses: actions/setup-node@v6
        with:
          node-version-file: web/.nvmrc
          cache: yarn
          cache-dependency-path: web/yarn.lock

      - name: Enable corepack
        run: corepack enable

      - name: Install dependencies
        run: cd web && yarn install --immutable

      - name: Build TypeScript docs
        working-directory: web
        run: yarn typedoc

      - name: Upload docs artifact
        uses: actions/upload-artifact@v6
        with:
          name: docs
          path: |
            target/doc
            web/docs
          retention-days: 7
```

**Step 2: Commit**

```bash
git add .github/workflows/ci.yml
git commit -m "ci: add build-docs job for Rust and TypeScript documentation"
```

---

## Task 5: Replace deploy-pages with Cloudflare Deployment

**Files:**
- Modify: `.github/workflows/ci.yml`
- Delete: `.github/workflows/cleanup-pr-previews.yml`

**Step 1: Replace deploy-pages job**

Replace the entire `deploy-pages` job (lines 731-895) with:

```yaml
  # ============================================================
  # JOB 3: Deploy to Cloudflare Pages
  # ============================================================
  deploy-cloudflare:
    name: Deploy to Cloudflare Pages
    runs-on: ubuntu-latest
    needs: [integration-tests, storybook-build, build-docs]
    if: always() && needs.integration-tests.result != 'cancelled'
    permissions:
      contents: read
      deployments: write
      pull-requests: write
    steps:
      - name: Create site directory
        run: mkdir -p site

      - name: Download coverage report
        uses: actions/download-artifact@v6
        with:
          name: coverage-report
          path: site/coverage
        continue-on-error: true

      - name: Download Storybook
        uses: actions/download-artifact@v6
        with:
          name: storybook-static
          path: site/storybook
        continue-on-error: true

      - name: Download Playwright report
        uses: actions/download-artifact@v6
        with:
          name: playwright-test-results
          path: artifacts/playwright
        continue-on-error: true

      - name: Download docs
        uses: actions/download-artifact@v6
        with:
          name: docs
          path: artifacts/docs
        continue-on-error: true

      - name: Organize artifacts
        run: |
          # Playwright report is nested
          if [ -d artifacts/playwright/playwright-report ]; then
            mv artifacts/playwright/playwright-report site/playwright
          fi

          # Rust docs
          if [ -d artifacts/docs/target/doc ]; then
            mv artifacts/docs/target/doc site/rust-docs
          fi

          # TypeScript docs
          if [ -d artifacts/docs/web/docs ]; then
            mv artifacts/docs/web/docs site/ts-docs
          fi

          rm -rf artifacts

      - name: Generate landing page
        run: |
          cat > site/index.html <<'EOF'
          <!DOCTYPE html>
          <html>
          <head>
            <title>Tiny Congress - Developer Resources</title>
            <style>
              body { font-family: system-ui, -apple-system, sans-serif; max-width: 700px; margin: 50px auto; padding: 20px; line-height: 1.6; }
              h1 { color: #1a1a1a; border-bottom: 2px solid #e5e5e5; padding-bottom: 10px; }
              h2 { color: #333; margin-top: 30px; }
              ul { list-style: none; padding: 0; }
              li { margin: 8px 0; }
              a { color: #0066cc; text-decoration: none; padding: 12px 18px; display: inline-block; background: #f5f5f5; border-radius: 6px; transition: background 0.2s; }
              a:hover { background: #e5e5e5; }
              .timestamp { color: #666; font-size: 0.85em; margin-top: 40px; padding-top: 20px; border-top: 1px solid #e5e5e5; }
            </style>
          </head>
          <body>
            <h1>Tiny Congress</h1>
            <h2>API Documentation</h2>
            <ul>
              <li><a href="rust-docs/tinycongress_api/">Rust API Docs</a></li>
              <li><a href="ts-docs/">TypeScript Docs</a></li>
            </ul>
            <h2>Reports</h2>
            <ul>
              <li><a href="coverage/">Coverage Report</a></li>
              <li><a href="storybook/">Storybook</a></li>
              <li><a href="playwright/">Playwright Report</a></li>
            </ul>
            <p class="timestamp">Generated from commit ${GITHUB_SHA:0:7} on $(date -u +"%Y-%m-%d %H:%M UTC")</p>
          </body>
          </html>
          EOF

      - name: Deploy to Cloudflare Pages
        id: deploy
        uses: cloudflare/pages-action@v1
        with:
          apiToken: ${{ secrets.CLOUDFLARE_API_TOKEN }}
          accountId: ${{ secrets.CLOUDFLARE_ACCOUNT_ID }}
          projectName: tiny-congress
          directory: site
          gitHubToken: ${{ secrets.GITHUB_TOKEN }}

      - name: Add PR comment with report links
        if: github.event_name == 'pull_request'
        uses: marocchino/sticky-pull-request-comment@v2
        with:
          header: pages-reports
          message: |
            ## CI Reports

            | Report | Link |
            |--------|------|
            | Rust Docs | [${{ steps.deploy.outputs.url }}/rust-docs/tinycongress_api/](${{ steps.deploy.outputs.url }}/rust-docs/tinycongress_api/) |
            | TS Docs | [${{ steps.deploy.outputs.url }}/ts-docs/](${{ steps.deploy.outputs.url }}/ts-docs/) |
            | Coverage | [${{ steps.deploy.outputs.url }}/coverage/](${{ steps.deploy.outputs.url }}/coverage/) |
            | Storybook | [${{ steps.deploy.outputs.url }}/storybook/](${{ steps.deploy.outputs.url }}/storybook/) |
            | Playwright | [${{ steps.deploy.outputs.url }}/playwright/](${{ steps.deploy.outputs.url }}/playwright/) |

            <sub>Generated from commit ${{ github.sha }}</sub>
```

**Step 2: Delete cleanup-pr-previews.yml**

Cloudflare handles preview cleanup automatically.

```bash
rm .github/workflows/cleanup-pr-previews.yml
```

**Step 3: Commit**

```bash
git add .github/workflows/ci.yml
git rm .github/workflows/cleanup-pr-previews.yml
git commit -m "ci: migrate from GitHub Pages to Cloudflare Pages

- Replace deploy-pages job with deploy-cloudflare
- Add docs artifacts to deployment
- Remove cleanup-pr-previews.yml (CF handles automatically)
- Update PR comment with docs links"
```

---

## Task 6: Squash into Two Logical Commits

**Step 1: Interactive rebase to squash**

We now have 5 commits that need to become 2:
1. Commits 1-3 → "feat: add documentation generation"
2. Commits 4-5 → "ci: migrate to Cloudflare Pages with docs"

```bash
git rebase -i HEAD~5
```

Reorder and squash:
```
pick <commit1> chore(web): add typedoc dependency
squash <commit2> chore(web): add typedoc configuration
squash <commit3> chore: add documentation build recipes to justfile
pick <commit4> ci: add build-docs job for Rust and TypeScript documentation
squash <commit5> ci: migrate from GitHub Pages to Cloudflare Pages
```

**Step 2: Edit commit messages**

First commit message:
```
feat: add documentation generation for Rust and TypeScript

- Add TypeDoc dependency and configuration
- Add justfile recipes: docs, docs-rust, docs-ts
- Configure TypeDoc to exclude tests and internal modules

Part of #215
```

Second commit message:
```
ci: migrate to Cloudflare Pages with source-generated docs

- Replace gh-pages branch deployment with Cloudflare Pages
- Add build-docs CI job for cargo doc and TypeDoc
- Deploy rust-docs and ts-docs alongside existing reports
- Update PR comments with documentation links
- Remove cleanup-pr-previews.yml (CF handles automatically)

Closes #215
```

**Step 3: Verify commits**

```bash
git log --oneline -3
```

Expected: Two new commits plus the design doc commit

---

## Task 7: Push and Verify

**Step 1: Push branch**

```bash
git push origin 215-generate-docs
```

**Step 2: Monitor CI**

```bash
gh run watch --branch 215-generate-docs
```

**Step 3: Verify Cloudflare deployment**

After CI passes, check:
- PR comment appears with links
- Click each link to verify:
  - `/rust-docs/tinycongress_api/` - Rust API documentation
  - `/ts-docs/` - TypeScript documentation
  - `/coverage/` - Coverage report
  - `/storybook/` - Storybook
  - `/playwright/` - Playwright report

**Step 4: Test local recipes**

```bash
just docs
```

Expected: Both doc sets build successfully

---

## Task 8: Cleanup (Post-Merge)

After PR is merged:

**Step 1: Delete gh-pages branch**

```bash
git push origin --delete gh-pages
```

**Step 2: Update repo settings (manual)**

Go to: Repository → Settings → Pages
- Note: No changes needed since we're using Cloudflare, not GitHub Pages

---

## Verification Checklist

- [ ] `just docs` builds Rust and TypeScript docs locally
- [ ] `just docs-rust` opens Rust docs in browser
- [ ] CI build-docs job passes
- [ ] Cloudflare deployment succeeds
- [ ] PR comment shows all report links
- [ ] Landing page at root URL shows all links
- [ ] Rust docs are browsable at `/rust-docs/tinycongress_api/`
- [ ] TypeScript docs are browsable at `/ts-docs/`
- [ ] PR preview URLs work (unique per commit)
