# Room Types Architecture

**Date:** 2026-03-17
**Status:** Design brief — not yet scoped into tickets
**Context:** The current system has one room shape (polling with multi-dimensional sliders) and three eligibility gates (endorsed_by, community, congress). Room "type" today means "who can enter," not "what happens inside." The vision requires fundamentally different interaction models sharing a common container.

---

## The Two Meanings of "Room Type"

**Room Template** — "I want a polling room about education priorities." Same interaction model, different configuration. A user creates this. No code needed.

**Room Module** — "I want a slow exchange room." Fundamentally different interaction model, data schema, reducer, UI. A developer creates this. Requires code (or a hosted service).

| | Template | Module |
|---|---|---|
| Created by | Any user | Developer |
| What varies | Topic, dimensions, eligibility, governance knobs | Entire interaction model |
| Examples | "Budget priorities" poll, "School board" poll | Polling, slow exchange, ranking, report synthesis |
| Requires deploy? | No | Yes (or federation) |

---

## Container / Module Separation

```
Room = Container (eligibility, governance, lifecycle, anchor)
  └── RoomModule = Plugin (interaction model, data schema, reducer, UI)
```

**Container handles** (shared across all room types):
- Identity verification and trust graph eligibility (constraint system — already built)
- Room lifecycle (open/closed/archived — already built)
- Room anchor (trust computation reference point)
- Governance policy (future)
- Room directory and discovery

**Module handles** (unique per room type):
- What data is collected (polls? messages? rankings?)
- How input is structured (sliders? text? pairwise?)
- How results are computed (mean? synthesis? ranking algorithm?)
- What the UI renders

### Module interface (sketch)

```
RoomModule {
    // Identity
    metadata() → { name, description, version, config_schema }

    // Lifecycle — container calls these
    on_create(config) → initial_state
    on_join(user, eligibility) → Result
    on_leave(user) → Result

    // Interaction — the core differentiator
    submit(user, input) → Result       // vote, message, ranking pick
    validate_input(input) → Result     // module-specific validation

    // Reduction — the philosophical core
    reduce(all_inputs) → ReducedOutput  // transparent, auditable aggregation

    // UI
    input_schema() → FormSchema         // what the user fills in
    output_schema() → DisplaySchema     // how results are rendered
}
```

**The `reduce()` function is the philosophical core of TC.** Every room type, regardless of interaction model, must produce a computationally reducible output — a transparent, auditable function from individual inputs to statistical summary. This is what separates TC from a forum or chat. A slow exchange "reduces" to: exchange happened, both parties confirmed receipt, retention terms met. A polling room reduces to distributions per dimension. A ranking room reduces to an aggregate ranking. The reduce function IS the room's value proposition.

---

## Known Room Types

| Room Type | Participants | Interaction | Pacing | Output | Status |
|-----------|-------------|-------------|--------|--------|--------|
| **Polling** | Many | Multi-dimensional sliders | Async, no limit | Aggregate distributions | Built (current) |
| **Slow exchange** | 2 | Consent-gated messages | Enforced delay (1wk+) | Private record + completion status | Vision doc, not built |
| **Ranking** | Many | Pairwise comparison picks | Async | Aggregate ranked list | Mentioned in domain model |
| **Report synthesis** | Many → 1 (AI) | Structured input per batch window | Batch windows | Published artifact | Vision doc, not built |
| **Deliberation** | Many | Threaded discussion + reactions | Async | Structured summary | Conceptual |

---

## Self-Hosting / Federation Model

The vision: users can create rooms from existing types (templates) or self-host room type implementations.

**Three-layer model:**
1. **TC Platform** = identity + trust + room directory (centrally operated)
2. **Room Service** = implements a room module (anyone can run)
3. **Federation protocol** = the module interface above, as an API contract

A community group runs their own "budget deliberation" room service plugged into TC's trust graph. They control the interaction design, data retention, and reducer. TC provides verified identity and eligibility.

**Implementation path:**

| Phase | What | When |
|-------|------|------|
| **Now** | Single binary, polling is the only module. Container/module boundary is implicit. | Current state |
| **Next** | Extract module interface from polling code. Build slow exchange as second module, same binary. Proves the abstraction. | Post-demo |
| **Later** | `config_schema` as template system — users create rooms by picking a module + filling in its config. | Growth phase |
| **Eventually** | Module interface becomes an HTTP API contract. Room services can run externally. | Federation |

---

## Relationship to Existing Architecture

### Constraints are container-level, not module-level

The three constraint types (endorsed_by, community, congress) determine eligibility — who can participate. This is a container concern. The module determines what participants do once inside. A polling room and a slow exchange room can both use the `community` constraint.

### Room anchor is a container concept

Every room needs a trust anchor for eligibility computation. This is currently missing from the schema (cast_vote passes None — see #665). The anchor should be a first-class field on the room container, not buried in constraint_config JSONB.

### Lifecycle queue is container-level

The `rooms__lifecycle_queue` (migration 17) manages poll rotation — but this is module-specific behavior leaked into the container. In the module model, the polling module would own its own lifecycle (when polls close, what activates next). The container lifecycle is simpler: room open/closed/archived.

### Poll tables are module-specific

`rooms__polls`, `rooms__poll_dimensions`, `rooms__votes` are all polling-module tables. In the module model, each module owns its data schema. The slow exchange module would have different tables (messages, retention policies, exchange status).

---

## Open Design Questions

See open questions doc Q31 for the consolidated list. Key decisions:

1. **When users say "make my own room" — template (config) or module (code)?** Both, but templates are the 99% case. Module creation is developer work.
2. **Unit of self-hosting: whole TC instance or single room service?** Single room service plugging into central identity/trust. Like Mastodon for interaction models, not for identity.
3. **Private reducers?** Can a room type have a proprietary aggregation algorithm with auditable inputs/outputs? Tension with transparency principle.
4. **Module-specific data isolation.** Each module owns its tables — but how? Schema prefixes? Separate databases? In-binary modules share the same Postgres; federated modules own their storage entirely.
