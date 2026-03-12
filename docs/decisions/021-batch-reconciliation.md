# ADR-021: Batch Reconciliation — 24-Hour Action Cadence

## Status
Proposed

## Context

The trust engine currently processes actions in near-real-time: a user endorses someone, the action is enqueued, the worker picks it up, and the graph recomputes from the actor's anchor. This creates several problems as the system scales:

- **Impulsive actions have immediate consequences.** A rage-endorsement or rage-denouncement takes effect before the user can reconsider.
- **Ordering matters.** If Alice endorses Bob and Bob endorses Carol in the same minute, the results depend on which action the worker processes first. This creates non-deterministic outcomes.
- **Downstream replication is continuous.** Any service consuming trust scores must handle a stream of incremental updates, making data consistency harder.
- **Attention cost is unbounded.** Users feel pressure to check their trust status frequently if changes propagate immediately.

The deliberation platform's thesis is that good collective decisions require reflection, not speed. The trust infrastructure should embody this same principle.

## Decision

### Actions are declared intentions

During the day, users submit trust actions (endorse, revoke, denounce) as **declarations of intent**. These are recorded in the action queue but are not processed immediately.

Users can modify or retract their declared actions before the reconciliation window closes. This creates a natural "cooling off" period.

### Daily batch reconciliation

At a fixed time each day (e.g., 00:00 UTC), all pending actions are processed as a single atomic batch:

1. **Collect** all pending actions since last reconciliation.
2. **Validate** each action against current state (budget checks, slot availability, duplicate detection).
3. **Apply** all valid actions to the trust graph.
4. **Recompute** trust scores globally (all anchors, all paths).
5. **Publish** the new daily trust snapshot.
6. **Notify** downstream consumers (rooms) that a new snapshot is available.

Actions that fail validation during the batch are rejected and the user is notified.

### Exception: room admission is immediate

Room eligibility checks evaluate the **latest available snapshot**, not a pending future state. When a user's trust position changes (after a batch), room access updates immediately. The batch cadence applies to the trust graph computation, not to room policy evaluation.

This means: if today's batch gives you `diversity >= 2`, you can enter a Congress room immediately after reconciliation — you don't wait for the next batch.

### Why 24 hours

The cadence is a design parameter, not a technical constraint. 24 hours was chosen because:

- It aligns with human daily rhythms — declare your intentions today, see results tomorrow.
- It forces intentionality. You can't endorse someone and immediately leverage the changed graph. You must plan ahead.
- It makes coordinated attacks visible. A burst of endorsements from a cluster will all appear in the same batch, making the pattern detectable.
- It simplifies replication. Downstream services sync once per day against a canonical snapshot, like a financial daily close.

The cadence may be adjusted (12h, 48h) based on observed user behavior, but the principle of batched reconciliation is the decision.

## Consequences

### Positive
- Users can retract impulsive actions before they take effect.
- The trust graph has a canonical daily state — no ambiguity about "which version" a room is evaluating.
- Batch processing sees all of today's actions together: mutual endorsements (A→B and B→A) resolve simultaneously rather than creating ordering dependencies.
- Full graph recomputation is tractable in a batch (NxN anchors for N users) whereas incremental recomputation under continuous updates requires careful scoping.
- Downstream consumers (rooms, visualizations, analytics) get a clean daily checkpoint to sync against.
- Coordinated attacks (Sybil swarms, graph pruning) are more visible when all actions in a window are analyzed together.

### Negative
- New users don't see endorsement effects until the next batch. The onboarding UX must communicate this clearly ("your trust score will update tomorrow").
- The system must handle the "pending state" UX: showing users what they've declared, what's pending, and what the current snapshot says.
- Batch processing creates a daily load spike. The reconciliation job must complete within a time budget.
- Edge case: a user endorses someone and expects immediate room access change. The UI must explain why nothing changed yet.

### Neutral
- The existing `trust__action_queue` table already stores actions before processing. The change is when and how they're consumed, not the storage model.
- The `trust__score_snapshots` table with `computed_at` timestamps naturally supports daily snapshots.
- The `TrustWorker` batch processing pattern (`process_batch()`) is already implemented — the change is triggering it on a schedule rather than continuously.

## Alternatives considered

### Real-time processing (current implementation)
- Actions take effect as soon as the worker picks them up.
- Simple, low latency, familiar.
- Rejected because it undermines intentionality, creates ordering dependencies, and makes replication harder. Speed is not a virtue for trust decisions.

### Event-sourced with projection
- Store all actions as an immutable event log, project the current graph state on demand.
- Architecturally clean but significantly more complex to implement and query.
- Rejected for now. The batch model captures the same benefits (reproducibility, auditability) with simpler infrastructure. Could revisit if the action log becomes a first-class product feature.

### Variable cadence per action type
- Endorsements batch daily, revocations process immediately (safety concern).
- Adds complexity and creates inconsistent mental models for users.
- Rejected. If a revocation is urgent, the room layer can respond to a "pending revocation" signal without the trust graph recomputing.

## References
- [ADR-003: pgmq job queue](003-pgmq-job-queue.md) — queue infrastructure for action storage and batch triggering
- [ADR-017: Two-layer trust architecture](017-two-layer-trust-architecture.md) — the layer separation this cadence applies to
- [ADR-020: Reputation scarcity](020-reputation-scarcity.md) — the action budgets consumed during each batch
- TRD §3.4 (Materialization Strategy) — original discussion of when to recompute
