# PR Hygiene Plan

**Date:** 2026-03-15
**Purpose:** Clean up branch/PR state to match the current planning reality.

---

## 1. Design workspace consolidation

**Current state:** `test/624-trust-simulation-harness` and `sim/trust-simulation-design-workspace` are identical (zero diff). PR #676 tracks the latter. The former has no PR (old #643 was closed).

**Action:**
- Push latest commits to `sim/trust-simulation-design-workspace` (5 commits behind current work)
- Update PR #676 description to reflect current contents (red team, expansion concepts, Q30, anchor reframe)
- No new PR needed for `test/624-trust-simulation-harness`

## 2. Resolve #684 / #691 overlap

**Current state:** Both PRs address issue #680 (scale simulation). Each has 1 commit with different HEADs. #684 is the framework PR referenced in all `.plan/` docs. #691 appears to be a follow-on for scale graph generation.

**Action:**
- Verify #691 builds on #684 or is independent
- If #691 supersedes #684: close #684 with a note pointing to #691
- If #691 is a follow-on: update #691 description to clarify relationship, keep both
- Whichever survives: add to post-demo milestone (scale hardening, not demo-critical)

## 3. Milestone simulation PRs

**Current state:** PRs #678, #679, #684 are complete simulation work with no milestone.

**Action:**
- #678 (Phase 1+2, denouncer validation): ready for review, no milestone → mark ready, assign demo milestone (validates ADR-024 which is demo-critical)
- #679 (Phase 3, time decay): draft, no milestone → mark ready, assign demo milestone (validates ADR-025)
- #684 or #691 (Phase 4, scale sim): draft, no milestone → assign post-demo milestone (scale hardening)

## 4. Clean stale branches

**Candidates for deletion (0 commits ahead of master or superseded):**

| Branch | Reason | Remote too? |
|--------|--------|-------------|
| `sim/open-questions-workspace` | Superseded by `sim/trust-simulation-design-workspace` | Yes |
| `test/624-simulation-merge` | Dead, 0 ahead | Yes |
| `test/624-trust-graph-simulation` | Dead, closed PR #625 | Yes |
| `test/624-trust-simulation-harness-clean` | Dead, 0 ahead | Yes (if exists) |
| `worktree-agent-a0151693` | Ephemeral agent branch | Local only |
| `worktree-agent-a0ba0b05` | Ephemeral agent branch | Local only |
| `worktree-agent-a33ed6cd` | Ephemeral agent branch | Local only |
| `worktree-agent-a3f4d2fd` | Ephemeral agent branch | Local only |
| `worktree-agent-a903db54` | Ephemeral agent branch | Local only |
| `worktree-agent-ae0c3a51` | Ephemeral agent branch | Local only |
| `worktree-agent-ae0e4d3f` | Ephemeral agent branch | Local only |
| `worktree-sim-open-questions` | Ephemeral worktree branch | Local only |

**Safety:** Verify each has 0 unique commits before deleting. Use `git log master..<branch> --oneline` for each.

## 5. Update PR #676 description

PR #676's description is stale (lists contents from before the red team model, expansion concepts, anchor reframe, and Q30 additions).

**Updated contents table should include:**
- `2026-03-13-open-questions.md` — 30 questions, 17 resolved, Q30 (verifiers as graph participants)
- `2026-03-13-scale-readiness-matrix.md` — Tier-gated evidence requirements
- `2026-03-13-trust-robustness-overview.md` — Mechanism proofs + scale analysis
- `2026-03-13-red-team-threat-model.md` — Attacker-first analysis, 12 vectors
- `2026-03-13-trust-expansion-concepts.md` — Capabilities by scale tier
- `2026-03-13-anchor-problem-statement.md` — Launch-accepted, multi-anchor Tier 3+
- `_archived/` — 3 completed simulation plans

---

## Execution order

1. Push to `sim/trust-simulation-design-workspace` (unblocks everything)
2. Update PR #676 description
3. Resolve #684/#691 overlap
4. Milestone PRs #678, #679, and the surviving scale PR
5. Delete stale branches (safe to batch)

## Not in scope

- Merging any simulation PRs (needs review)
- Creating new PRs for demo-critical work (separate task)
- ADR status updates (separate task)
