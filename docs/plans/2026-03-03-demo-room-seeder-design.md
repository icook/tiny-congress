# LLM-Powered Demo Room Seeder

**Date:** 2026-03-03
**Status:** Approved

## Problem

The demo instance starts with an empty database after migrations. Visitors see no rooms, polls, or results — making the demo feel lifeless. We need continuously generated content so the demo always has active rooms with populated poll results that visitors can also interact with.

## Solution

A Rust binary (`tc-seed`) in the existing workspace, deployed as a K8s CronJob on the demo instance. It uses OpenRouter to generate room topics, poll questions, and voting dimensions, then seeds them directly into the database alongside simulated votes from synthetic accounts.

## Architecture

```
┌─────────────────────────────────────────────┐
│  K8s CronJob (every 30 min)                 │
│  ┌────────────────────────────────────────┐  │
│  │  tc-seed binary                        │  │
│  │                                        │  │
│  │  1. Query DB: count active rooms/polls │  │
│  │  2. If below threshold:                │  │
│  │     → Call OpenRouter LLM              │  │
│  │     → Parse structured response        │  │
│  │     → Insert rooms/polls/dimensions    │  │
│  │     → Activate polls                   │  │
│  │  3. Ensure synthetic accounts exist    │  │
│  │  4. Cast simulated votes on new polls  │  │
│  └──────────┬────────────┬───────────────┘  │
│             │            │                   │
│        ┌────▼────┐  ┌────▼──────┐           │
│        │ Postgres │  │ OpenRouter│           │
│        │ (direct) │  │   API    │           │
│        └─────────┘  └──────────┘           │
└─────────────────────────────────────────────┘
```

### Key decisions

- **Direct DB access** via the repo layer — avoids the crypto auth challenge entirely (no need to sign HTTP requests with Ed25519 keys).
- **CronJob, not long-running worker** — simpler lifecycle, K8s handles scheduling and retries. Runs every 30 min, checks state, tops up if needed, exits.
- **Create-only in v1** — no lifecycle management (closing polls, archiving rooms). Old content accumulates; lifecycle management can be added later.
- **Prompt-configured topics** — the LLM system prompt is stored in a ConfigMap so the demo operator can change the topic flavor (civic governance, tech community, etc.) without code changes.
- **LLM generates everything** — room names, descriptions, poll questions, and custom dimension names/descriptions per poll.

## Configuration

| Variable | Source | Purpose |
|---|---|---|
| `DATABASE_URL` | Secret | Postgres connection |
| `OPENROUTER_API_KEY` | Secret | LLM API key |
| `OPENROUTER_MODEL` | ConfigMap | Model to use (e.g., `anthropic/claude-sonnet-4-6`) |
| `SEED_TARGET_ROOMS` | ConfigMap | Target number of active rooms (default: 5) |
| `SEED_VOTES_PER_POLL` | ConfigMap | Synthetic votes per poll (default: 15) |
| `SEED_SYSTEM_PROMPT` | ConfigMap | Customizable prompt for topic flavor |

## LLM Interaction

Single structured prompt per run. The system prompt (from ConfigMap) sets the topic domain. The user prompt requests N rooms, each with 2-3 polls and 3-5 dimensions per poll. Response parsed as JSON.

Example LLM output shape:

```json
{
  "rooms": [{
    "name": "Downtown Transit Expansion",
    "description": "Should the city invest in...",
    "polls": [{
      "question": "Which corridor should be prioritized?",
      "description": "Rate each factor...",
      "dimensions": [
        {"name": "Ridership Impact", "description": "Expected daily riders", "min": 0, "max": 10},
        {"name": "Cost Efficiency", "description": "Bang for the buck", "min": 0, "max": 10},
        {"name": "Equity", "description": "Serves underserved areas", "min": 0, "max": 10}
      ]
    }]
  }]
}
```

## Synthetic Accounts

~20 synthetic demo accounts created on first run with deterministic seeds (idempotent):
- Usernames: `demo_voter_01` through `demo_voter_20`
- All endorsed with `identity_verified` topic
- Votes distributed with slight randomness using a seeded RNG for reproducibility

## File Layout

| Component | Location |
|---|---|
| Rust binary | `service/src/bin/seed.rs` (or `service/src/seed/` module) |
| OpenRouter client | `service/src/seed/llm.rs` |
| CronJob manifest | `kube/app/templates/cronjob-seed.yaml` |
| ConfigMap for prompt | `kube/app/templates/configmap-seed.yaml` |
| Secret for API key | homelab-gitops encrypted secret |
