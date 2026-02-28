# Design: Source-Generated Docs on GitHub Pages

**Ticket:** #215 - [Docs] Publish source-generated docs to GitHub Pages
**Date:** 2025-12-20
**Status:** Approved

## Overview

Publish browsable API documentation generated from source code (Rust and TypeScript) to a public URL. Contributors can navigate types, modules, and public APIs without cloning the repo.

## Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Scope | Rust + TypeScript | Both are core to the project; TypeDoc adds minimal complexity |
| Directory structure | Flat (`/rust-docs/`, `/ts-docs/`) | Matches existing pattern (`/coverage/`, `/storybook/`) |
| Hosting | Cloudflare Pages | No git storage overhead, native PR previews, 25MB/file limit works for Playwright videos |
| PR previews | Yes, for all reports including docs | Enables iteration; CF provides automatic preview URLs |
| Landing page | Simple static HTML | 11ty was vestigial; just need links |
| Sequencing | Single PR, two commits | Migration first, then docs |

## Architecture

### Deployment Flow

```
CI Build Jobs                    Cloudflare Pages
┌─────────────────┐             ┌─────────────────┐
│ integration-tests│────┐       │                 │
│ (coverage,       │    │       │  Production     │
│  playwright)     │    │       │  tiny-congress. │
├─────────────────┤    ▼       │  pages.dev      │
│ storybook-build │──►Assemble──►                 │
├─────────────────┤    Site    │  PR Previews    │
│ build-docs      │────┘       │  abc123.tiny-   │
│ (rust, ts)      │            │  congress.      │
└─────────────────┘            │  pages.dev      │
                                └─────────────────┘
```

### Site Structure

```
site/
├── index.html          # Landing page with links to all resources
├── coverage/           # Unified coverage report (existing)
├── storybook/          # Storybook component library (existing)
├── playwright/         # Playwright test report (existing)
├── rust-docs/          # cargo doc output
│   ├── tinycongress_api/
│   ├── tc_crypto/
│   └── ...
└── ts-docs/            # TypeDoc output
    └── ...
```

## Implementation

### Commit 1: Migrate to Cloudflare Pages

**Remove gh-pages branch approach:**
- Delete vestigial 11ty scaffolding from gh-pages
- Remove `deploy-pages` job that commits to gh-pages branch
- Remove PR cleanup workflow for gh-pages

**Add Cloudflare Pages deployment:**

```yaml
# .github/workflows/ci.yml

deploy-cloudflare:
  name: Deploy to Cloudflare Pages
  runs-on: ubuntu-latest
  needs: [integration-tests, storybook-build]
  if: always() && needs.integration-tests.result != 'cancelled'
  permissions:
    contents: read
    deployments: write
    pull-requests: write
  steps:
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

    - name: Organize Playwright report
      run: |
        if [ -d artifacts/playwright/playwright-report ]; then
          mv artifacts/playwright/playwright-report site/playwright
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
            body { font-family: system-ui, sans-serif; max-width: 600px; margin: 50px auto; padding: 20px; }
            h1 { color: #333; }
            ul { list-style: none; padding: 0; }
            li { margin: 10px 0; }
            a { color: #0066cc; text-decoration: none; padding: 10px 15px; display: inline-block; background: #f5f5f5; border-radius: 5px; }
            a:hover { background: #e5e5e5; }
            .timestamp { color: #666; font-size: 0.9em; margin-top: 30px; }
          </style>
        </head>
        <body>
          <h1>Tiny Congress</h1>
          <h2>Reports</h2>
          <ul>
            <li><a href="coverage/">Coverage Report</a></li>
            <li><a href="storybook/">Storybook</a></li>
            <li><a href="playwright/">Playwright Report</a></li>
          </ul>
          <p class="timestamp">Generated: $(date -u +"%Y-%m-%d %H:%M:%S UTC")</p>
        </body>
        </html>
        EOF

    - name: Deploy to Cloudflare Pages
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
          | Coverage | ${{ steps.deploy.outputs.url }}/coverage/ |
          | Storybook | ${{ steps.deploy.outputs.url }}/storybook/ |
          | Playwright | ${{ steps.deploy.outputs.url }}/playwright/ |

          <sub>Generated from commit ${{ github.sha }}</sub>
```

**Secrets required (already configured):**
- `CLOUDFLARE_ACCOUNT_ID`: `927a5895b62dabc04eab63dcd8bbdecd`
- `CLOUDFLARE_API_TOKEN`: (set via gh secret)

### Commit 2: Add Documentation Generation

**Add build-docs job:**

```yaml
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

**Add TypeDoc configuration (`web/typedoc.json`):**

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
  "readme": "none"
}
```

**Add TypeDoc dev dependency:**

```bash
cd web && yarn add -D typedoc
```

**Update deploy job to include docs:**

```yaml
- name: Download docs
  uses: actions/download-artifact@v6
  with:
    name: docs
    path: artifacts/docs
  continue-on-error: true

- name: Organize docs
  run: |
    if [ -d artifacts/docs/target/doc ]; then
      mv artifacts/docs/target/doc site/rust-docs
    fi
    if [ -d artifacts/docs/web/docs ]; then
      mv artifacts/docs/web/docs site/ts-docs
    fi
    rm -rf artifacts/docs
```

**Update landing page to include docs:**

```html
<h2>API Documentation</h2>
<ul>
  <li><a href="rust-docs/tinycongress_api/">Rust API Docs</a></li>
  <li><a href="ts-docs/">TypeScript Docs</a></li>
</ul>
<h2>Reports</h2>
...
```

**Update PR comment:**

```markdown
| Rust Docs | ${{ steps.deploy.outputs.url }}/rust-docs/tinycongress_api/ |
| TS Docs | ${{ steps.deploy.outputs.url }}/ts-docs/ |
```

### Local Development

**Add to justfile:**

```just
# =============================================================================
# Documentation
# =============================================================================

# Build and preview all docs locally
docs:
    cargo doc --workspace --no-deps
    cd web && yarn typedoc
    @echo "Rust docs: target/doc/tinycongress_api/index.html"
    @echo "TS docs: web/docs/index.html"

# Build and open Rust docs
docs-rust:
    cargo doc --workspace --no-deps --open

# Build and open TypeScript docs
docs-ts:
    cd web && yarn typedoc
    open web/docs/index.html
```

**Update web/.gitignore:**

```
docs/
```

## Cleanup

After migration is verified working:

1. Delete gh-pages branch: `git push origin --delete gh-pages`
2. Update repo Settings → Pages → Source to "GitHub Actions" (or leave as-is since we're using CF)
3. Remove `cleanup-pr-previews.yml` workflow (CF handles this automatically)

## Acceptance Criteria

- [ ] HTML docs are generated from source code (no manual copy)
- [ ] Docs are published to Cloudflare Pages on merges to master
- [ ] PR previews include docs at unique URLs
- [ ] Landing page links to each generated doc set
- [ ] Build runs in CI without secrets issues and uses pinned tool versions
- [ ] Local preview steps are documented (`just docs`, `just docs-rust`, `just docs-ts`)
- [ ] PR comments include links to rust-docs and ts-docs

## Risks & Mitigations

| Risk | Mitigation |
|------|------------|
| Cloudflare outage | Docs are non-critical; acceptable downtime |
| 25MB file limit | Playwright videos may need compression; monitor |
| TypeDoc config issues | Start simple, iterate based on output quality |
| Breaking existing PR comments | Test on a single PR first before merging |
