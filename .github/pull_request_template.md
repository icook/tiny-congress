## Context
<!-- WHY does this change exist? What problem does it solve or what goal does it serve?
     For feature work, link the design doc or plan that motivated it. -->

## Changes made
<!-- WHAT changed, grouped by logical unit. For each group:
     - What was the decision and why (not just what files were touched)
     - If code was removed, how did you confirm zero callers/references? -->
-

## Risks & Caveats
<!-- What could go wrong? What doesn't this PR do that a reviewer might expect?
     Examples: "Tests skip gracefully if no seed data exists",
     "WASM-dependent flows only run in CI, not local dev"
     Delete this section if genuinely N/A. -->

## Testing
- [ ] `just test` (backend + frontend unit)
- [ ] `just lint` (all linters)
- [ ] E2E (CI-only / local — specify which)
- [ ] Manual verification (describe what you did)

## Plan Graduation
<!-- Delete this section if no .plan/ files exist on this branch -->
- [ ] If this PR removes `.plan/` files, durable decisions are captured in the appropriate docs:
  - Architectural decisions → `docs/decisions/NNN-*.md` (ADR)
  - Domain model changes → `docs/domain-model.md`
  - API contracts / interfaces → `docs/interfaces/*.md`
  - Operational procedures → `docs/playbooks/*.md`
  - UX patterns → `docs/style/*.md`

## Linked Issue
- Closes #

## AI tooling used
