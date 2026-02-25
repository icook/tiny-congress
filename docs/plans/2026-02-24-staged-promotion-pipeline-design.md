# Design: Staged Promotion Pipeline with LLM Exploratory Testing

**Date:** 2026-02-24
**Status:** Draft

## Overview

Replace the current single-deploy model with a three-stage promotion pipeline: staging → demo → prod. Each promotion is gated by automated validation — LLM-driven exploratory testing for staging→demo, and human acceptance for demo→prod.

The core idea: LLMs are most effective when feedback loops are tight and acceptance criteria are machine-verifiable. This pipeline makes the LLM both a development tool and a deployment gatekeeper.

## Current State

```
master push → CI → deploy-gitops (writes digests to demo HelmRelease)
```

One environment ("demo"), deployed automatically on every master merge after CI passes. No post-deploy validation. No promotion gates.

## Target State

```
master push
  → CI (lint, test, build, E2E, scan)              ~15 min
  → deploy to STAGING (automatic)                   ~2 min
  → LLM exploratory testing against staging          ~5-10 min
      ├─ green → promote to DEMO                     ~2 min
      └─ red   → block promotion, open GitHub issue
  → DEMO available for human dogfooding
  → manual promotion to PROD (future)
```

## Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Number of stages | 3 (staging, demo, prod) | Staging for automated testing, demo for humans, prod for future |
| Staging deploy trigger | Every master merge | Staging should always reflect HEAD |
| Promotion gate | LLM exploratory pass | Automated quality gate before humans see it |
| Exploration scope | Diff-focused + smoke | Deep testing on changes, quick health check on everything else |
| Exploration runtime | GitHub Actions | Reuses existing CI infra, no cluster-side LLM keys needed |
| LLM browser access | Playwright MCP | Already used in dev tooling; proven browser automation |
| Environment hosting | Same homelab cluster, namespace isolation | Reuses existing Helm chart, Flux manages both |
| Blocking behavior | Promotion blocked on failure, deploy not rolled back | Staging stays on HEAD regardless; demo stays on last-good |
| Failure notification | GitHub issue with evidence | Screenshots, console logs, reproduction steps |

## Architecture

### Environments

| Environment | Purpose | Deployed by | URL | Lifecycle |
|-------------|---------|-------------|-----|-----------|
| **Staging** | LLM testing ground | CI (automatic on master) | staging.ibcook.com | Always tracks master HEAD |
| **Demo** | Human dogfooding | CI (promotion after green exploration) | demo.ibcook.com | Only updated on successful promotion |
| **Prod** | Production users | Manual (future) | ibcook.com | Manual promotion from demo |

All three run in the same homelab cluster in separate namespaces (`tc-staging`, `tc-demo`, `tc-prod`), each with their own postgres instance and ingress.

### Gitops Repository Structure

```
clusters/sauce/workloads/tiny-congress/
  helmrelease-staging.yaml    # tracks master HEAD (current helmrelease-demo.yaml renamed)
  helmrelease-demo.yaml       # promoted digests only
  helmrelease-prod.yaml       # future: manually promoted
```

The existing `deploy-gitops` CI job writes to `helmrelease-staging.yaml` instead of `helmrelease-demo.yaml`. A new promotion step copies staging digests to `helmrelease-demo.yaml` when exploration passes.

### Promotion Flow

```
┌──────────────┐     ┌──────────────┐     ┌──────────────┐
│   STAGING    │     │     DEMO     │     │     PROD     │
│              │     │              │     │              │
│ auto-deploy  │────►│  promoted    │────►│   manual     │
│ every master │ LLM │  on green    │human│  promotion   │
│    merge     │gate │  exploration │gate │   (future)   │
└──────────────┘     └──────────────┘     └──────────────┘
```

### CI Workflow Changes

**Existing workflow (`ci.yml`) changes:**
- Rename `deploy-gitops` → targets staging instead of demo
- Update gitops file path from `helmrelease-demo.yaml` to `helmrelease-staging.yaml`

**New workflow (`explore-staging.yml`):**
- Triggered by `workflow_run` on CI completion (master, success)
- Runs the LLM exploratory agent against the staging environment
- On success, promotes digests from staging to demo in the gitops repo
- On failure, opens a GitHub issue with evidence

```yaml
# .github/workflows/explore-staging.yml
name: Explore Staging

on:
  workflow_run:
    workflows: ["CI"]
    types: [completed]
    branches: [master]

jobs:
  explore:
    name: LLM exploratory testing
    if: github.event.workflow_run.conclusion == 'success'
    runs-on: ubuntu-latest
    steps:
      - name: Checkout
        uses: actions/checkout@v6

      - name: Wait for staging deployment
        run: |
          # Poll staging health endpoint until the new version is live
          EXPECTED_SHA="${{ github.event.workflow_run.head_sha }}"
          timeout 300 bash -c '
            until curl -sf https://staging.ibcook.com/health | grep -q "'$EXPECTED_SHA'"; do
              sleep 10
            done
          '

      - name: Get diff since last promotion
        id: diff
        run: |
          # Compare master HEAD against what's currently in demo
          # (read demo digest from gitops repo, map back to commit)
          ...

      - name: Run LLM exploratory agent
        env:
          ANTHROPIC_API_KEY: ${{ secrets.ANTHROPIC_API_KEY }}
        run: |
          # Claude with Playwright MCP, pointed at staging URL
          # Agent receives: diff, schema, route inventory
          # Agent produces: structured report
          ...

      - name: Promote to demo (on success)
        if: success()
        run: |
          # Copy staging digests to helmrelease-demo.yaml in gitops repo
          ...

      - name: Open issue (on failure)
        if: failure()
        run: |
          gh issue create \
            --title "Exploratory testing failed for ${{ github.event.workflow_run.head_sha }}" \
            --body "..." \
            --label "type/bug,priority/high"
```

## LLM Exploration Agent

### Inputs

The agent receives a structured prompt with:

1. **Diff summary** — files changed between the last-promoted commit and current master HEAD
2. **GraphQL schema** — from `web/schema.graphql`, so it knows available operations
3. **Route inventory** — extracted from the React router config, so it knows available pages
4. **Staging URL** — the base URL to test against

### Two-phase execution

**Phase 1 — Targeted exploration (~3-5 min)**
Based on the diff, the agent focuses on what changed:
- Modified GraphQL resolvers → exercise those queries/mutations via the UI
- Changed React pages/components → navigate to those routes, interact with them
- Modified API endpoints → verify responses through the frontend
- Schema changes → test affected flows end-to-end

The agent interacts via Playwright MCP: navigating pages, clicking elements, filling forms, observing results.

**Phase 2 — Smoke sweep (~1-2 min)**
Quick health check of every known route:
- Load the page
- Check for HTTP errors (4xx, 5xx in network tab)
- Check for console errors
- Verify the page renders (not blank/error state)
- Move on — no deep interaction

**Skip condition** — If the diff only touches docs, CI config, or non-app files, skip both phases and auto-promote.

### Output

Structured JSON report:

```json
{
  "sha": "abc123",
  "timestamp": "2026-02-24T12:00:00Z",
  "phases": {
    "targeted": {
      "flows_tested": [...],
      "pass": true,
      "findings": []
    },
    "smoke": {
      "routes_checked": [...],
      "pass": true,
      "findings": []
    }
  },
  "screenshots": ["screenshot-1.png", ...],
  "overall": "pass"
}
```

Each finding includes: severity (critical/warning/info), description, screenshot reference, console logs, and reproduction steps.

## Implementation Sequence

### Phase 1: Multi-environment gitops setup
- Create `helmrelease-staging.yaml` and `helmrelease-demo.yaml` in gitops repo
- Set up `tc-staging` and `tc-demo` namespaces with ingress
- Wildcard DNS and TLS for `*.ibcook.com` (or explicit records)
- Update `deploy-gitops` to target staging instead of demo
- Verify: staging deploys on master push, demo stays unchanged

### Phase 2: Health-gated promotion (no LLM yet)
- New workflow `explore-staging.yml` triggered by CI completion
- Waits for staging to be healthy (version-aware health check)
- Runs a simple smoke test (curl every known route, check for 200s)
- On success, promotes staging digests to demo
- This validates the promotion plumbing without LLM complexity

### Phase 3: LLM exploratory agent
- Add Claude + Playwright MCP to the exploration workflow
- Build the agent prompt with diff, schema, and route context
- Start with targeted exploration only (phase 1 of the two-phase agent)
- Structured report output as workflow artifact
- GitHub issue creation on failure

### Phase 4: Refinement
- Add smoke sweep (phase 2 of the agent)
- Tune skip conditions (docs-only, CI-only diffs)
- Add historical tracking (are explorations finding real issues?)
- Consider demo→prod promotion gate (manual or scheduled)

## Open Questions

1. **Health endpoint versioning** — Does `/health` currently return the git SHA? If not, how do we know staging has the new version deployed before testing?
2. **LLM cost** — Each exploration will consume Claude API tokens. Rough estimate: ~$0.50-2.00 per run depending on depth. Acceptable?
3. **Gitops repo write access** — The promotion step needs write access to the gitops repo (same deploy key as staging). Should promotion use the same key or a separate one?
4. **Flaky explorations** — LLM-driven testing may be non-deterministic. Strategy: retry once on failure before opening an issue? Or always open and let humans triage?
5. **Database state** — Should staging and demo share a database, or fully isolated? Isolated means demo data is stable for dogfooding, but staging data resets frequently.

## See Also

- `.github/workflows/ci.yml` — existing CI pipeline
- `docs/playbooks/gitops-cd-setup.md` — deploy key and webhook setup
- `kube/app/` — Helm chart (shared by all environments)
