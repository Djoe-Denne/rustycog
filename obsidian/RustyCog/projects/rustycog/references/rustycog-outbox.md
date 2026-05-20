---
title: >-
  RustyCog Outbox
category: references
tags: [reference, rustycog, events, database, visibility/internal]
sources:
  - rustycog/rustycog-outbox/src/lib.rs
  - rustycog/rustycog-outbox/src/recorder.rs
  - rustycog/rustycog-outbox/src/dispatcher.rs
  - rustycog/rustycog-outbox/src/migration.rs
  - Manifesto/infra/src/transaction.rs
  - Manifesto/setup/src/app.rs
summary: >-
  rustycog-outbox bridges RustyCog DB transactions and RustyCog Events dispatch without making rustycog-events depend on database code.
provenance:
  extracted: 0.86
  inferred: 0.10
  ambiguous: 0.04
created: 2026-04-26T13:36:00Z
updated: 2026-05-20T14:05:00Z
---

# RustyCog Outbox

`rustycog::outbox` (historically `rustycog-outbox`) is the integration module that connects [[projects/rustycog/references/rustycog-db]] and [[projects/rustycog/references/rustycog-events]] while keeping `rustycog::events` transport-only. Services opt into the outbox migration explicitly, record domain events inside their own write transaction, and let an embedded dispatcher publish through [[entities/event-publisher]] after commit.

## Architectural Flow

```mermaid
flowchart TD
    uow["Service Unit Of Work"] --> txn["Db Write Transaction"]
    txn --> domainRows["Domain Rows"]
    txn --> outboxRows["rustycog_outbox_events"]
    outboxRows --> dispatcher["Embedded Outbox Dispatcher"]
    dispatcher --> publisher["EventPublisher"]
    publisher --> queue["SQS Or Kafka"]

    class uow,txn,domainRows,outboxRows,dispatcher,publisher,queue internal-link;
```

## Project Creation Sequence

```mermaid
sequenceDiagram
    participant API as Manifesto Project Use Case
    participant UOW as ProjectCreationUnitOfWork
    participant DB as Postgres Transaction
    participant Outbox as OutboxRecorder
    participant Worker as OutboxDispatcher
    participant Queue as EventPublisher

    API->>UOW: create project + ProjectCreated event
    UOW->>DB: BEGIN
    UOW->>DB: insert project, owner member, owner permissions
    UOW->>Outbox: record(ProjectCreated)
    Outbox->>DB: insert pending outbox row
    UOW->>DB: COMMIT
    Worker->>DB: claim pending row
    Worker->>Queue: publish stored event
    Worker->>DB: mark published or failed
```

## Key Semantics

- The outbox table stores `event_id`, `event_type`, `aggregate_id`, event version, event payload JSON, metadata JSON, status, attempts, lock ownership, retry time, and the last publish error.
- `OutboxRecorder::record()` accepts any SeaORM `ConnectionTrait`, so a service can pass the same `DatabaseTransaction` used for domain rows.
- The dispatcher claims retryable rows by moving them to `publishing`, increments attempts, sets `locked_by` / `locked_until`, publishes through the injected `EventPublisher`, and then marks the row `published` or `failed`.
- Delivery is at-least-once: `event_id` is the durable idempotency key, and downstream consumers still need duplicate-safe handling.
- Manifesto is the first rollout slice: `ProjectCreated` is recorded in the project-creation transaction and dispatched asynchronously after commit.

## Failure Behavior

```mermaid
flowchart LR
    commit["Domain Commit Succeeds"] --> pending["Outbox Row Pending"]
    pending --> publish{"Publish Works?"}
    publish -->|yes| published["Mark Published"]
    publish -->|no| failed["Mark Failed"]
    failed --> retry["Schedule next_attempt_at"]
    retry --> pending

    rollback["Domain Transaction Fails"] --> noDomain["No Domain Rows"]
    rollback --> noOutbox["No Outbox Row"]
```

If the queue is down, the project commit still succeeds and the outbox row remains retryable. If the database transaction rolls back, neither the project rows nor the event intent survive.

## Related Notes

- [[concepts/event-driven-microservice-platform]]
- [[projects/rustycog/references/rustycog-db]]
- [[projects/rustycog/references/rustycog-events]]
- [[projects/rustycog/references/index]]
