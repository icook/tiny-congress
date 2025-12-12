# ADR-003: Use pgmq for job queue

## Status
Accepted

## Context
The application needs background job processing for:
- Async vote tallying
- Email notifications
- Data aggregation

Options range from external message brokers (Redis, RabbitMQ) to database-backed queues.

## Decision
Use [pgmq](https://github.com/tembo-io/pgmq) - a PostgreSQL extension that provides message queue functionality directly in the database.

Jobs are stored in Postgres tables with visibility timeout, retry logic, and dead-letter queue support.

## Consequences

### Positive
- No additional infrastructure (reuses existing Postgres)
- Transactional consistency (enqueue in same transaction as data changes)
- Simple operations (backup, monitoring via existing Postgres tools)
- ACID guarantees on message delivery

### Negative
- Postgres becomes a bottleneck for high-throughput queues
- Must ensure pgmq extension loaded before migrations run
- Less ecosystem tooling compared to Redis/RabbitMQ

### Neutral
- Queue performance sufficient for current scale (~1000 jobs/minute)
- Can migrate to external broker later if needed

## Alternatives considered

### Redis + Sidekiq pattern
- Battle-tested, high throughput
- Rejected: Additional infrastructure, no transactional enqueue with DB writes

### RabbitMQ
- Full-featured message broker
- Rejected: Operational complexity disproportionate to needs

### Database polling (DIY)
- Simple SELECT ... FOR UPDATE SKIP LOCKED
- Rejected: pgmq provides this pattern with better ergonomics

## References
- pgmq documentation: https://github.com/tembo-io/pgmq
- `dockerfiles/Dockerfile.postgres` - extension installation
- `service/migrations/` - queue table setup
