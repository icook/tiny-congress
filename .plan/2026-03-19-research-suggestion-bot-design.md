# Research Suggestion Bot — Design Brief

**Ticket:** #852
**Parent:** #757 (steerable research)

## Goal

Replace the `research_suggestion` stub with a real LLM+Exa pipeline that turns user suggestions into evidence on existing poll dimensions.

## Decisions

1. **Suggestions are poll-scoped.** The table gets a `poll_id` column; routes move to `/rooms/{room_id}/polls/{poll_id}/suggestions`.
2. **Bot picks the dimension.** Users don't choose a dimension — the bot maps the suggestion to the best-fit existing dimension (or creates a new one if nothing fits). Optional dimension scoping is a future enhancement.
3. **Single-task pipeline.** `research_suggestion` does everything in one pgmq task: reformulate → search → synthesize → insert evidence → update suggestion. No multi-task coordination.
4. **LLM-reformulated search.** One LLM call turns the suggestion + poll context into 2-3 targeted Exa search queries. Better results than raw suggestion text.
5. **2-4 evidence items** per suggestion. Pro/con split determined by sources, not forced.
6. **Rate limit stays room-scoped.** `room_id` column remains for rate-limit queries. 3/day per user per room.

## Pipeline

```
User submits suggestion (poll-scoped)
  → content filter gate (NoopFilter for now)
  → rate limit check (3/day/user/room)
  → INSERT as 'queued'
  → process_suggestions scheduler claims via FOR UPDATE SKIP LOCKED
  → enqueues pgmq 'research_suggestion' task

research_suggestion task:
  1. Read suggestion_text + poll context (topic, existing dimensions)
  2. LLM: generate 2-3 search queries from suggestion + context
     → emit 'query_generation' trace step
  3. Parallel Exa searches (one per query)
     → emit 'exa_search' trace step per query
  4. LLM: synthesize search results into 2-4 evidence claims (pro/con + source)
     → emit 'llm_synthesis' trace step
  5. Pick best-fit dimension (or create new one)
  6. Insert evidence via evidence_repo::insert_evidence
  7. Update suggestion: status='completed', evidence_ids=[...], processed_at=now()
  8. Return Some(poll_id) for trace linkage
```

## Data Model Changes

```sql
ALTER TABLE rooms__research_suggestions
  ADD COLUMN poll_id UUID NOT NULL REFERENCES rooms__poll_polls(id);

CREATE INDEX idx_suggestions_poll_status
  ON rooms__research_suggestions(poll_id, status, created_at);
```

## Route Changes

- Old: `GET/POST /rooms/{room_id}/suggestions`
- New: `GET/POST /rooms/{room_id}/polls/{poll_id}/suggestions`

## Error Handling

- On failure: `fail_suggestion(pool, id)` sets status='failed'. Worker's `fail_trace` handles the trace. No automatic retry.
- Exa/LLM failures are logged and surfaced as trace step failures before bailing.

## Frontend Changes

- `SuggestionFeed` moves into poll detail view, receives `pollId`
- API client updated for poll-scoped route
- Query hooks updated with `pollId` parameter

## Non-goals (future tickets)

- Optional dimension picker (#851 role-based limits may interact)
- Real content filter (#853)
- Streaming/partial progress
- Retry mechanism
