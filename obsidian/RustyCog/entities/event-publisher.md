---
title: EventPublisher
category: entities
tags: [rustycog, events, messaging, visibility/internal]
sources:
  - rustycog/rustycog-events/src/event.rs
  - rustycog/rustycog-events/src/lib.rs
summary: EventPublisher is RustyCog's async publication interface for Kafka, no-op, and SQS fanout across configured destination queues.
provenance:
  extracted: 0.9
  inferred: 0.05
  ambiguous: 0.05
created: 2026-04-15T17:15:56.0808743Z
updated: 2026-04-25T10:53:00Z
---

# EventPublisher

`EventPublisher<TError>` is the publishing abstraction behind RustyCog event transport factories.

## Key Ideas

- `EventPublisher` is the async publication interface (`publish`, `publish_batch`, `health_check`) used by services and adapters.
- Factory wiring selects Kafka, SQS, or no-op implementations from `QueueConfig`.
- The abstraction keeps call sites transport-agnostic while leaving transport-specific setup in one place.
- For SQS, one `publish` call now fans the same serialized event out to every queue resolved from `SqsConfig` for that event type.
- `publish_batch` preserves the same fanout semantics by grouping batch entries by destination queue before sending them.
- Queue-targeted variants build on top of this base publisher contract, but service code should rely on `SqsConfig` destination lists rather than duplicating fanout in service adapters.

## Sources

- [[projects/rustycog/references/rustycog-events]]
- [[entities/domain-event]]
- [[entities/queue-config]]
