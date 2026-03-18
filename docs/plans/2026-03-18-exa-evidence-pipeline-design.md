# Exa Evidence Pipeline Design

**Goal:** Replace single-shot OpenRouter evidence generation with Exa search + LLM synthesis for better-sourced, cheaper evidence cards.

**Context:** Battery testing showed Perplexity deep research produces excellent sourced evidence but costs ~$1/company ($25/seed cycle). Claude + OpenRouter web plugin is cheap ($0.02) but single-shot with shallow sourcing. DIY Exa search + synthesis hits ~$0.05-0.17/company with full source control.

## Architecture

Two-step pipeline replacing `generate_company_evidence()`:

1. **Parallel Exa searches** — 5 concurrent `POST https://api.exa.ai/search` calls, one per dimension (e.g. "Sysco Corporation labor practices"). Each returns top 5 results with inline text highlights via `contents: { text: { maxCharacters: 3000 } }`. All 5 run in parallel via `futures::join_all`.

2. **LLM synthesis** — One OpenRouter call with all search results in context. Prompt instructs the model to extract structured pro/con evidence cards citing sources. Output: existing `CompanyEvidence` struct. Claims include source URLs from Exa results.

## Config

- `SIM_EXA_API_KEY` — required for evidence generation
- `SIM_OPENROUTER_API_KEY` — still needed for synthesis LLM call
- `SIM_EVIDENCE_MODEL` — optional synthesis model override (default: Haiku for cost)

## Cost Profile

| Component | Cost/Company |
|---|---|
| 5 Exa searches (text inline) | $0.035 |
| Haiku synthesis | ~$0.015 |
| **Total (Haiku)** | **~$0.05** |
| **Total (Sonnet)** | **~$0.17** |

Exa free tier: 1,000 requests/month = 200 companies before paying for search.

## Files

| File | Change |
|---|---|
| `service/src/sim/llm.rs` | Replace `generate_company_evidence()` with Exa search + synthesis; add Exa API types |
| `service/src/sim/config.rs` | Add `exa_api_key`, `evidence_model` |
| `service/src/sim/brand.rs` | Update callers (signature stays same) |
| `service/src/bin/sim.rs` | Log new config fields |

## Decision Record

- **Why not Perplexity deep research?** $1/company too expensive for demo. Returns prose, not JSON.
- **Why not dzhng/deep-research?** Overkill — we know exactly what to search for (5 fixed dimensions). No need for recursive query refinement.
- **Why Exa over Tavily?** Exa is already OpenRouter's search backend. Cheaper per-query ($0.007 vs ~$0.008). Free tier is generous.
- **Why Haiku default?** Structured JSON extraction from well-sourced results doesn't need frontier reasoning. Haiku is 60x cheaper than Sonnet for output tokens.
