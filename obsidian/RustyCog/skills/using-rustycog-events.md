---
title: Using RustyCog Events
category: skills
tags: [rustycog, events, skills, visibility/internal]
sources:
  - rustycog/rustycog-events/src/lib.rs
  - rustycog/rustycog-events/src/event.rs
  - rustycog/rustycog-config/src/lib.rs
summary: Operational workflow for defining domain events and wiring RustyCog publishers/consumers, including SQS fanout queue lists.
provenance:
  extracted: 0.9
  inferred: 0.05
  ambiguous: 0.05
created: 2026-04-15T17:15:56.0808743Z
updated: 2026-04-25T10:53:00Z
---

# Using RustyCog Events

Use this guide when integrating `<!-- [[projects/rustycog/references/rustycog-events]] -->` in service setup.

## Workflow

- Define event payload types that satisfy `DomainEvent` (type, IDs, timestamp, version, payload JSON, metadata).
- Load `QueueConfig` from service config and build publisher/consumer via factory helpers.
- For SQS fanout, configure `[queue] default_queues = [...]` and `[queue.queues] event_type = ["queue-a", "queue-b"]`; RustyCog publishes the event to every destination queue.
- Use `publish` for single events and `publish_batch` for transactional/event-burst cases.
- For queue-targeted scenarios, use `create_multi_queue_event_publisher()` for the shared adapter surface, but keep destination routing in `SqsConfig` so service code does not duplicate fanout logic.
- Consumers created from SQS config poll every configured queue independently through one shared `EventHandler`.
- Add transport health checks to startup diagnostics to detect silent no-op fallbacks.

## Common Pitfalls

- Assuming queue setup failure always stops startup; factories can degrade to no-op mode.
- Mixing transport-specific event naming conventions without a shared event-type contract.
- Adding the same destination queue multiple times for one event; `SqsConfig` deduplicates, but duplicated config is still a signal to clean up intent.

## Sources

- <!-- [[projects/rustycog/references/rustycog-events]] -->
- <!-- [[entities/domain-event]] -->
- <!-- [[concepts/event-driven-microservice-platform]] -->
