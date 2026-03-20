# 743: Trace Viewer UI Design Brief

## Status: In Progress

## Goal

Upgrade the BotTraceViewer component (shipped in PR #793) to match the full spec from issue #743: three-level progressive disclosure with relative timestamps and proper cache status rendering.

## Current State (PR #793)

- Two-state (collapsed/expanded) viewer
- Collapsed: "Bot generated . N steps . $X.XXX" + status badge
- Expanded: Timeline with step type, model, tokens, cost, latency
- Cache badge shows "cached" whenever cache object has any keys (broken: shows cached even for `{ litellm_proxy_hit: false }`)
- No relative time display ("2m ago")
- No full-detail third level (per-step LLM output)
- No search-step-specific rendering (query, results_count)
- Step types shown raw ("llm_call", "exa_search") instead of human-readable

## Changes

### 1. Collapsed row improvements
- Add relative time: "Bot generated . 4 steps . $0.012 . 2m ago"
- Use the existing `timeAgo` pattern from `TimestampText.tsx` (inline helper, not the full component)

### 2. Cache badge logic fix
- Only show "cached" when at least one cache value is truthy
- `{ litellm_proxy_hit: false }` = not cached
- `{ nginx_hit: true }` = cached
- `{ openrouter_prompt_tokens_cached: 400 }` = cached (truthy number)

### 3. Step type display
- `llm_call` -> "LLM Call"
- `exa_search` -> "Web Search"
- Fallback: capitalize and replace underscores

### 4. Search step rendering
- Show query text when present
- Show results count when available

### 5. Full detail level (third state)
- Each step has its own expand/collapse for full detail
- Shows `output_summary` as the full LLM output text in a code block or pre-wrapped text

### 6. Visual polish
- Total cost in collapsed row formatted to 3 decimal places (already done)
- Per-step cost formatted to 4 decimal places (already done)
- Wrap in a Card with "Research Trace" header

## Files Modified

- `web/src/engines/polling/components/BotTraceViewer.tsx` - Main component improvements
- `web/src/engines/polling/components/BotTraceViewer.test.tsx` - Unit tests

## Out of Scope

- Backend changes (API shape is sufficient)
- New dependencies
- Changes to trace data schema
