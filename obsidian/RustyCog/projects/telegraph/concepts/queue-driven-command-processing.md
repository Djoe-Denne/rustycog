---
title: Queue-Driven Command Processing
category: concepts
tags: [events, commands, queue, rust, visibility/internal]
sources:
  - Telegraph/setup/src/app.rs
  - Telegraph/application/src/command/factory.rs
  - Telegraph/application/src/usecase/event_processing.rs
  - Telegraph/infra/src/event/consumer.rs
summary: Telegraph routes SQS events through a rustycog command service so async consumers and HTTP handlers can share typed use-case orchestration.
provenance:
  extracted: 0.76
  inferred: 0.15
  ambiguous: 0.09
created: 2026-04-14T18:18:24.0602572Z
updated: 2026-04-19T11:38:52.5746779Z
---

# Queue-Driven Command Processing

`[[projects/telegraph/telegraph]]` uses the same high-level command pattern as HTTP-first services, but its most interesting entrypoint is a queue consumer rather than a route handler. That makes Telegraph a useful counterexample to the current IAMRusty-heavy command pages: the `[[projects/rustycog/references/rustycog-command]]` runtime is flexible enough to serve both request/response and async event workflows.

## Key Ideas

- `TelegraphEventHandler` implements `rustycog_events::EventHandler`, receives queue-delivered domain events, and converts each one into a typed `ProcessEventCommand`.
- `TelegraphCommandRegistryFactory::create_telegraph_registry` registers both async event work (`process_event`) and synchronous notification read-model commands (`get_notifications`, `get_unread_count`, `mark_notification_read`) in one registry.
- `setup/src/app.rs` injects the same `GenericCommandService` into both `AppState` for HTTP handlers and `EventConsumer` for queue handling, so the service has one command runtime rather than two orchestration layers.
- `EventProcessingUseCase` converts `ProcessEventCommand` into `EventContext`, preserving event ID, event type, recipient identity, attempt count, and free-form metadata for downstream processors.
- `supports_event_type()` filters events against the configured `queues.*.events` list before a message is accepted for processing, so command dispatch is gated by runtime queue configuration rather than only by compile-time handler registration.
- Conflict to resolve: `IAMRusty` wires an explicitly configured retry-aware registry, while Telegraph currently builds its registry with plain `CommandRegistryBuilder::new()` and no visible service-specific retry binding. Both `rustycog` usage patterns exist in the live repo. ^[ambiguous]

## Open Questions

- Should queue-first services like Telegraph standardize on the same explicit registry retry configuration that IAMRusty binds through its command config? Conflict to resolve. ^[ambiguous]
- Queue events currently execute with a mostly empty `CommandContext`, while HTTP commands usually attach user metadata and richer request context. ^[inferred]

## Sources

- [[projects/telegraph/telegraph]] - Service where the queue-first variant is used concretely.
- [[concepts/command-registry-and-retry-policies]] - Broader cross-service view of registry wiring and retry differences.
- [[projects/rustycog/references/rustycog-command]] - Crate-level command runtime behind the shared registry and service facade.
- [[projects/telegraph/references/telegraph-event-processing]] - End-to-end queue consumer to processor pipeline.
- [[projects/telegraph/references/telegraph-service]] - Composition-root context for the shared command service.
