---
name: plan-graduation
description: Use when finishing a branch that has .plan/ files, before merging to master — reads plans and diffs, suggests which docs need updating, and scaffolds the graduation
---

# Plan Graduation

## Overview

When a feature branch ships, `.plan/` files must be stripped (CI enforces this). But decisions, domain concepts, and operational knowledge shouldn't vanish — they graduate to permanent docs.

**Core principle:** Read the plan + the branch diff. Suggest what's durable. Scaffold the docs. Strip the plans.

## When to Use

- Branch has `.plan/*.md` files (other than README.md)
- PR is approaching merge to master
- CI `check-no-plans` job is failing
- `finishing-a-development-branch` skill detects `.plan/` files

## The Process

### Step 1: Identify Plans

```bash
find .plan -name '*.md' ! -name 'README.md' 2>/dev/null
```

If none found, skip — no graduation needed.

### Step 2: Read Plans + Diff

For each `.plan/` file:
1. Read the full plan document
2. Read the branch diff against master: `git diff master...HEAD --stat`
3. Understand what was decided, what was built, what changed

### Step 3: Suggest Graduation Targets

For each plan, evaluate against these categories:

| Question | If yes → | Target |
|----------|----------|--------|
| Was an architectural choice made (A over B, with rationale)? | ADR | `docs/decisions/NNN-*.md` |
| Were new domain concepts introduced? | Domain model update | `docs/domain-model.md` |
| Was a new API contract or interface established? | Interface doc | `docs/interfaces/*.md` |
| Was operational knowledge discovered (how to deploy, test, debug)? | Playbook | `docs/playbooks/*.md` |
| Was a UX pattern or design rationale established? | Style doc | `docs/style/*.md` |
| Is nothing worth preserving? | Clean strip | Delete `.plan/` file |

**Present suggestions to the user:**
```
Scanning .plan/example-brief.md...

Detected graduation targets:
  - New domain concept: "trust distance" (weighted hop count)
    → Suggest: update docs/domain-model.md
  - Architecture decision: chose html5-qrcode over BarcodeDetector
    → Suggest: create docs/decisions/NNN-qr-scanning-library.md
  - No operational procedures detected
  - No new interfaces detected

Strip .plan/example-brief.md after graduation? [y/n]
```

### Step 4: Scaffold Documents

For each confirmed graduation target:

**ADR** — Use the template at `docs/decisions/000-template.md`. Fill in context, decision, and consequences from the plan.

**Domain model** — Add new concepts to the appropriate section of `docs/domain-model.md` with the same level of rigor as existing entries.

**Interface** — Follow the format of existing docs in `docs/interfaces/`.

**Playbook** — Follow the format of existing docs in `docs/playbooks/`.

### Step 5: Strip Plans

After all graduations are complete:
```bash
git rm .plan/*.md
# Keep .plan/README.md
git checkout HEAD -- .plan/README.md
```

## The Filter Question

Not every plan needs graduation. Ask: **"Would a developer six months from now wonder _why_ this was done this way?"**

- Yes → Graduate to appropriate doc
- No → Strip cleanly

Examples:
- Gap analysis fully resolved by implementation → strip
- "We chose library X because Y" → ADR
- "Trust distance means..." → domain model update
- "To test QR on mobile, you need..." → playbook

## Common Mistakes

**Graduating everything** — Implementation roadmaps and gap analyses that are fully resolved by the code don't need permanent docs. The code IS the documentation.

**Only creating ADRs** — Decisions are one type. Domain model updates, playbooks, and interface docs are equally important graduation targets.

**Losing the "why"** — The most valuable part of a plan is the rationale, not the implementation steps. When graduating, preserve the reasoning.

**Forgetting to strip** — CI will catch this, but don't rely on CI. Strip as part of the graduation commit.
