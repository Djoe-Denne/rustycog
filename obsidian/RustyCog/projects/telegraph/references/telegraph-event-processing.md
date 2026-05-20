---
title: Telegraph Event Processing
category: references
tags: [reference, events, communication, visibility/internal]
sources:
  - Telegraph/config/development.toml
  - Telegraph/setup/src/app.rs
  - Telegraph/infra/src/event/consumer.rs
  - Telegraph/infra/src/event/processors/mod.rs
  - Telegraph/infra/src/event/processors/email.rs
  - Telegraph/infra/src/event/processors/notification.rs
  - Telegraph/application/src/usecase/event_processing.rs
  - Telegraph/domain/src/service/communication_factory.rs
  - Telegraph/tests/user_signup_event_test.rs
  - Telegraph/tests/user_email_verified_event_test.rs
summary: Telegraph-specific event processing on top of RustyCog's shared queue and command layers, including config-gated routing, descriptor use, and email versus notification delivery.
provenance:
  extracted: 0.78
  inferred: 0.13
  ambiguous: 0.09
created: 2026-04-14T18:18:24.0602572Z
updated: 2026-04-19T12:08:26.9393504Z
---

# Telegraph Event Processing

This page assumes the shared queue and command runtime from `[[projects/rustycog/references/rustycog-events]]` and `[[projects/rustycog/references/rustycog-command]]`. It focuses on how `[[projects/telegraph/telegraph]]` turns those primitives into Telegraph-specific event handling.

## RustyCog Baseline

- `[[projects/rustycog/references/rustycog-events]]` explains the event envelope, queue consumer, and publisher abstractions that Telegraph consumes.
- `[[projects/rustycog/references/rustycog-command]]` explains the shared `GenericCommandService` runtime that the queue handler delegates into.
- `[[projects/telegraph/concepts/queue-driven-command-processing]]` and `[[projects/telegraph/concepts/descriptor-driven-communications]]` capture the two Telegraph-specific concepts layered on top of those shared crates.

## Service-Specific Differences

- `EventConsumer::new()` creates a `rustycog_events` consumer from `config.queue`, then wraps it in a Telegraph-specific handler that delegates every accepted message into `GenericCommandService`.
- `supports_event_type()` consults the configured `queues.*.events` lists before accepting a message type, which means runtime config decides whether an event is processed or discarded.
- `ProcessEventCommand` is the bridge between transport and business logic: `TelegraphEventHandler` creates it from the raw queue event, and `EventProcessingUseCase` converts it into an `EventContext` for domain processors.
- `CompositeEventProcessor` uses an `event_mapping` derived from queue config to choose which processor names to run per event type, then executes each applicable handler and logs partial failures before returning the first error.
- The email path builds `EmailCommunication` from descriptors and templates, while the notification path builds `NotificationCommunication`, persists it, and creates a delivery record with `delivery_method = "notification"`.
- Development config maps `user_signed_up` and `password_reset_requested` to `email`, while `user_email_verified` is mapped to `notification`, and the integration tests confirm those two behaviors end to end.
- The wider Telegraph model advertises SMS and broader direct-send shapes, but the live event processor composite currently wires only `email` and `notification`. ^[ambiguous]

## Adding a New Event Path

- Keep the transport contract stable first: the published event name, the `EventExtractor` fields, the `queues.*` routing entry, and the descriptor filename should all tell the same story.
- If the new event only changes content, prefer reusing the existing `email` and `notification` handlers and extend descriptors/templates instead of adding another orchestration layer.
- Add or update the descriptor and templates before touching processor code so the intended delivery modes and required variables are explicit.
- Extend `CompositeEventProcessor` wiring only when the event needs a genuinely new mode or side effect that the existing handlers cannot express cleanly.
- Prove the path end to end by publishing the real queue payload in tests and asserting the resulting SMTP output, notification row, or other persisted delivery effect.

## Open Questions

- Unsupported event types are logged and discarded when no configured queue claims them, but the current service surface does not expose a more explicit dead-letter or audit story in this code path. ^[inferred]
- Partial handler failures are logged in detail but collapsed into the first returned error, so multi-channel failure reporting is still lossy. ^[inferred]

## Sources

- [[projects/telegraph/telegraph]] - Project page for the service consuming these events.
- [[projects/telegraph/concepts/queue-driven-command-processing]] - Async command dispatch pattern behind the consumer.
- [[projects/telegraph/concepts/descriptor-driven-communications]] - Descriptor and template system used by the processors.
- [[projects/rustycog/references/rustycog-events]] - Crate-level event envelope and consumer abstractions used by Telegraph.
- [[projects/telegraph/references/telegraph-testing-and-smtp-fixtures]] - Tests that prove the main event paths.
