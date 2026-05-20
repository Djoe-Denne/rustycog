---
title: DomainEvent
category: entities
tags: [rustycog, events, messaging, visibility/internal]
sources:
  - rustycog/rustycog-events/src/event.rs
summary: DomainEvent is RustyCog's transport-neutral event contract covering event identity, aggregate linkage, versioning, payload serialization, and metadata.
provenance:
  extracted: 0.91
  inferred: 0.04
  ambiguous: 0.05
created: 2026-04-15T17:15:56.0808743Z
updated: 2026-04-15T22:10:00Z
---

# DomainEvent

`DomainEvent` is the core event contract in `[[projects/rustycog/references/rustycog-events]]`.

## Key Ideas

- `DomainEvent` is the transport-neutral contract for event identity, aggregate linkage, versioning, metadata, and payload serialization.
- It lets services define their own payload types while preserving one shared event envelope shape.
- Kafka/SQS/no-op implementations consume the same contract through RustyCog event adapters.
- Versioning and compatibility policies are discussed at concept level in `[[concepts/event-driven-microservice-platform]]`.

## Sources

- [[projects/rustycog/references/rustycog-events]]
- [[entities/event-publisher]]
- [[concepts/event-driven-microservice-platform]]
