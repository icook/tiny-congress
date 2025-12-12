# Agent Output Schema

## PR Compliance Block

Every agent-generated PR must include this YAML block at the end of the PR description.

### Schema

```yaml
# --- Agent Compliance ---
agent_compliance:
  docs_read:           # Required: list of documentation files read
    - AGENTS.md        # AGENTS.md is always required
    - <other docs>     # Optional additional docs
  constraints_followed: true | false  # Required: boolean
  files_modified:      # Required: list of files changed
    - path/to/file1
    - path/to/file2
  deviations:          # Required: list of rule exceptions
    - none             # Or explanation strings
```

### Validation rules

| Field | Required | Type | Validation |
|-------|----------|------|------------|
| `docs_read` | Yes | array[string] | Must include `AGENTS.md` |
| `constraints_followed` | Yes | boolean | Must be `true` or `false` |
| `files_modified` | Yes | array[string] | Must match actual git diff |
| `deviations` | Yes | array[string] | At least one entry (even if `none`) |

### Examples

#### Compliant PR
```yaml
# --- Agent Compliance ---
agent_compliance:
  docs_read:
    - AGENTS.md
    - docs/playbooks/adding-migration.md
  constraints_followed: true
  files_modified:
    - service/migrations/20240115_add_status.sql
    - service/src/models.rs
  deviations:
    - none
```

#### PR with deviation
```yaml
# --- Agent Compliance ---
agent_compliance:
  docs_read:
    - AGENTS.md
  constraints_followed: true
  files_modified:
    - skaffold.yaml
  deviations:
    - Modified skaffold.yaml per user request; testing-local-dev skill not run
```

#### Non-compliant (will fail CI)
```yaml
# Missing AGENTS.md in docs_read
agent_compliance:
  docs_read:
    - docs/playbooks/adding-migration.md
  constraints_followed: true
  files_modified: []
  deviations:
    - none
```

## Commit message format

```
<imperative summary> (max 50 chars)

<optional body explaining why>

🤖 Generated with [Claude Code](https://claude.com/claude-code)

Co-Authored-By: <Agent Name> <noreply@anthropic.com>
```

### Examples

```
Add vote counting endpoint

Implements GET /api/votes/count for dashboard metrics.
Uses cached aggregates to avoid N+1 queries.

🤖 Generated with [Claude Code](https://claude.com/claude-code)

Co-Authored-By: Claude Sonnet <noreply@anthropic.com>
```

## CI validation

The compliance block is validated by `.github/scripts/check-agent-compliance.sh`:

1. Detects `agent_compliance:` in PR body
2. Extracts YAML between `# --- Agent Compliance ---` and closing fence
3. Validates required fields present
4. Checks `AGENTS.md` in `docs_read`
5. Fails with actionable error if invalid

Human PRs (no compliance block) pass automatically.
