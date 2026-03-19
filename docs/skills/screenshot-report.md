---
name: screenshot-report
description: Use after completing UI changes to generate a visual report with Playwright screenshots showing before/after or current state of the changes
---

# Screenshot Report

Generate a visual change report with annotated Playwright screenshots after UI work is complete.

## When to Use

- After completing frontend UI changes (layout, styling, component rework)
- When a PR needs visual evidence of what changed
- When the user asks for a report, demo, or visual proof of UI changes

## Prerequisites

- Playwright MCP server connected
- App running and accessible (typically via `skaffold dev --port-forward` or `just dev`)
- Seeded data that exercises the changed components (create if needed)

## Procedure

### Step 1: Verify the app is reachable

```bash
# Check if frontend port-forward is active
lsof -ti:5173 2>/dev/null || echo "no frontend"
# Check if backend is responding
curl -s http://localhost:8080/rooms | head -c 100
```

If no frontend, start `just dev` or the appropriate dev command.

### Step 2: Ensure meaningful data exists

The report is only useful if the changed components render with real data. Check what's available:

```bash
curl -s http://localhost:8080/rooms | python3 -c "
import json, sys
for r in json.load(sys.stdin):
    print(f'{r[\"id\"]}: {r[\"name\"]}')
"
```

If needed, seed data via direct SQL through kubectl exec into the postgres pod:

```bash
kubectl get pods --all-namespaces | grep postgres  # find the pod
kubectl exec <pod> -n <ns> -- psql -U postgres -d tiny-congress -c "..."
```

Only seed the minimum data needed to exercise the changed components.

### Step 3: Create screenshots directory

```bash
mkdir -p screenshots
```

### Step 4: Navigate and capture

Use Playwright MCP tools in this order:

1. **`browser_navigate`** — Go to the page that shows the changes
2. **`browser_snapshot`** — Verify the page rendered correctly (check for expected elements)
3. **`browser_take_screenshot`** with `fullPage: true` — Capture the full page
4. **`browser_take_screenshot`** with `element`/`ref` — Capture focused shots of each changed area

**Naming convention:** `screenshots/NN-description.png` (e.g., `01-full-page.png`, `02-breadcrumbs.png`)

**Tips:**
- Use `browser_evaluate` with `window.scrollTo(0, 0)` before viewport screenshots
- For element screenshots, use the `ref` from `browser_snapshot` output
- Take a full-page screenshot first, then focused element shots
- If Chrome won't launch due to existing session, clear the cache: `rm -rf ~/Library/Caches/ms-playwright/mcp-chrome-*`

### Step 5: Write the report

Create `screenshots/REPORT.md` with this structure:

```markdown
# [Feature Name] Report

## Overview
One-sentence summary of the changes.

---

## 1. [Change Name]

**Before:** What it was.
**After:** What it is now.

![Description](filename.png)

**Files:** `path/to/changed/file.tsx` lines X-Y

---

## Files Changed

| File | Change |
|------|--------|
| ... | ... |

## Not Yet Addressed
Any deferred work or follow-up items.
```

### Step 6: Clean up

- Do NOT commit `screenshots/` or seeded test data
- Add `screenshots/` to `.gitignore` if it isn't already
- Remove seeded test data from the database if it was created for the report

## Completion Criteria

- [ ] Every changed UI area has at least one focused screenshot
- [ ] Full-page screenshot captures the overall layout
- [ ] `screenshots/REPORT.md` exists with annotated descriptions
- [ ] Report links each change to the source file(s)
- [ ] Screenshots render meaningful data, not loading/error states
