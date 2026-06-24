# Research Technique Experiment Harness — Design

> **Status:** Approved design. Ready for implementation planning.
> **Goal:** Benchmark a variety of research techniques against fixed seed questions to discover which approaches produce the best structured research output for different question types. Low/zero marginal cost per run.

## Cost Model

All LLM calls go through `claude -p` (Claude Code CLI oneshot) — subscription-only, no per-token API cost. Search is free (SearXNG, self-hosted). Scraping is free (ArchiveBox, self-hosted). A full benchmark run has near-zero marginal cost.

## Architecture

```
research/
├── harness.py              # Runner: loads seeds, dispatches techniques, collects results
├── primitives/
│   ├── search.py           # SearXNG wrapper
│   ├── scrape.py           # ArchiveBox CLI ingestion + disk read
│   └── llm.py              # claude -p oneshot wrapper with JSON output parsing
├── techniques/
│   ├── base.py             # Technique interface + ResearchGraph output type
│   ├── breadth_first.py
│   ├── iterative_deepening.py
│   ├── multi_perspective.py
│   ├── claim_then_verify.py
│   ├── map_reduce.py
│   ├── template_guided.py  # Mad-libs approach
│   └── autonomous_agent.py # Claude Code with research skill/prompt, multi-turn
├── eval/
│   ├── judge.py            # LLM-as-judge via claude -p
│   └── rubric.py           # Human evaluation helpers (print side-by-side, collect scores)
├── seeds/
│   └── questions.json      # Fixed benchmark questions
├── domains.json            # Allowed domain list for search result filtering
└── results/
    └── {run_id}.json       # One file per technique x question run
```

## Primitives

### Search — SearXNG (free, self-hosted)

Running in the lab. Harness sends queries via SearXNG's JSON API, filters results to allowed domains, returns URLs + snippets.

### Scrape — ArchiveBox (free, self-hosted)

**Ingestion:** `echo URLs | archivebox add` (batch, via subprocess).
**Retrieval:** Poll SQLite index for completion, then read `archive/<timestamp>/readability/*.json` — the `textContent` field gives clean extracted text (Mozilla Readability).
**Caching:** ArchiveBox is a natural cache. Re-running the benchmark on the same URLs does a disk read, no re-fetch.

### LLM — Claude Code CLI (subscription, no per-token cost)

Every LLM step (query generation, synthesis, evaluation) calls:
```bash
claude -p "{prompt}" --output-format json --max-turns 1 --allowedTools ""
```

For the autonomous agent technique, `--allowedTools` is opened up and `--max-turns` increased.

## Allowed Domains

Search results filtered to bot-friendly sources only. No domain triage step needed — the allowlist removes the confounding variable of scraping resilience.

**Categories:**
- Government/institutional: congress.gov, CBO, GAO, Federal Register, state legislatures
- Wire services: AP News, Reuters
- Public media: NPR, PBS, BBC, Al Jazeera
- Nonprofit investigative: ProPublica, The Intercept, The Markup, Bellingcat
- Open platforms: Wikipedia, Substack, WordPress/Hugo/Jekyll blogs, Ars Technica, EFF, Techdirt
- The Guardian (monitor for blocking changes)

## Techniques

Each technique is a Python function:
```python
def run(question: str, search: SearchClient, scrape: ScrapeClient, llm: LLMClient) -> ResearchGraph
```

| # | Technique | Description |
|---|-----------|-------------|
| 1 | **Breadth-first** | Generate N search queries upfront, scrape all results, synthesize once |
| 2 | **Iterative deepening** | Search -> read top result -> generate follow-up query -> repeat K times |
| 3 | **Multi-perspective** | Generate queries from 3 opposing viewpoints, search each independently, synthesize with explicit disagreement |
| 4 | **Claim-then-verify** | LLM generates claims from memory first, then searches to verify/refute each |
| 5 | **Map-reduce** | Search broadly, extract key facts per source independently, reduce into synthesis |
| 6 | **Template-guided** | Mad-libs template generates structured sub-questions, each researched independently |
| 7 | **Autonomous agent** | Claude Code with a research skill/prompt, multi-turn with web search + Playwright MCP |

Techniques 1-6 are structured pipelines. Technique 7 benchmarks raw agent capability against the hand-crafted pipelines.

## Seed Questions

Mix of temporally fresh (re-runnable with different results) and stable (reproducible comparison):

| # | Question | Type | Fresh? |
|---|----------|------|--------|
| 1 | "What happened in US politics in the last 24 hours?" | Current events, broad | Yes |
| 2 | "What are the arguments for and against ranked-choice voting?" | Policy, evergreen | No |
| 3 | "What legislation is currently moving through the US Congress?" | Factual, structured | Yes |
| 4 | "What are the main criticisms of [recent policy X]?" | Adversarial/critical | Semi |
| 5 | "Summarize the last week of local news in [city]" | Narrow scope, depth | Yes |

Questions 1, 3, 5 test technique stability across runs. Questions 2, 4 allow apples-to-apples comparison.

## Evaluation

### Automated (LLM-as-judge, every run)

A separate `claude -p` call scores each output on 1-5:

| Criterion | 1 (poor) | 5 (excellent) |
|-----------|----------|---------------|
| **Source diversity** | Single source / echo chamber | 5+ independent sources, different perspectives |
| **Claim grounding** | Claims fabricated or misrepresented | Every claim traceable to quoted source text |
| **Coverage** | Missed obvious perspectives | Comprehensive, including dissent |
| **Structure** | Monolithic paragraph blob | Clear claim->evidence pairs, maps to DAG nodes |
| **Actionability** | Vague summary, no specifics | Concrete facts, named actors, specific data |

Judge receives: original question, technique output, raw source texts. Scores each dimension with one-sentence justification.

### Human (comparative, for finalists)

Read side-by-side outputs and score:
- **Did I learn something?** — Novelty beyond 5 minutes of googling
- **Do I trust it?** — Does the synthesis feel honest vs. spin?
- **Would I share this?** — Useful enough to show someone in a room?
- **What did this technique miss that another caught?** — Qualitative, most valuable signal

### Per-run metadata (automatic)

- Wall-clock time
- Number of LLM calls (claude -p invocations)
- Number of search queries (SearXNG)
- Number of URLs archived (ArchiveBox)
- Total source text size (tokens, approximate)
- Output size (tokens)

### Output Format

Each run produces one JSON file:

```json
{
  "run_id": "20260330_143022_breadth_first_q1",
  "technique": "breadth_first",
  "question": "What happened in US politics in the last 24 hours?",
  "timestamp": "2026-03-30T14:30:22Z",
  "metadata": {
    "wall_clock_seconds": 45,
    "llm_calls": 3,
    "search_queries": 5,
    "urls_archived": 8,
    "source_tokens_approx": 12000,
    "output_tokens": 1500
  },
  "graph": {
    "nodes": [],
    "edges": []
  },
  "raw_sources": [],
  "eval_automated": {
    "source_diversity": {"score": 4, "reason": "..."},
    "claim_grounding": {"score": 3, "reason": "..."},
    "coverage": {"score": 4, "reason": "..."},
    "structure": {"score": 3, "reason": "..."},
    "actionability": {"score": 4, "reason": "..."}
  },
  "eval_human": null
}
```

## Relationship to Other Work

- `.plan/2026-03-28-research-dag-brief.md` — The DAG design brief. This harness validates which research techniques produce output that naturally decomposes into DAG nodes. The "structure" evaluation criterion directly tests this.
- `crates/tc-llm` — Existing Rust LLM/Exa client. Not used here (Python harness calls claude CLI directly). Learnings from the harness feed back into `tc-llm` when techniques graduate to production.
- `sim/brand.rs::battery()` — Existing battery test pattern in Rust. Conceptually similar (multi-config comparison runs). The Python harness is the research-domain equivalent.
