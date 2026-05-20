---
title: Building Event-Driven Notification Services
category: skills
tags: [events, services, rust, visibility/internal]
sources:
  - Telegraph/config/development.toml
  - Telegraph/setup/src/app.rs
  - Telegraph/infra/src/event/consumer.rs
  - Telegraph/domain/src/service/communication_factory.rs
  - Telegraph/tests/common.rs
summary: Build or extend a Telegraph-style service by combining queue-driven commands, descriptor-based template rendering, and end-to-end delivery tests.
provenance:
  extracted: 0.77
  inferred: 0.17
  ambiguous: 0.06
created: 2026-04-14T18:18:24.0602572Z
updated: 2026-04-19T11:38:52.5746779Z
---

# Building Event-Driven Notification Services

This skill captures the reusable Telegraph pattern: let queue transport trigger typed commands, render user-facing content from descriptors and templates, and keep the synchronous API focused on the stored read model rather than on direct delivery work.

## Workflow

- Start by modeling queue transport separately from per-event routing, so one config block answers "how do I reach SQS?" and another answers "which event types map to which delivery modes?"
- Build a `CommunicationFactory`-style layer that loads event descriptors and templates instead of hardcoding email or notification text directly in handlers.
- Route queue events through a `GenericCommandService` so async consumers and HTTP handlers share the same command names, error mappers, and use-case boundaries.
- Keep the live HTTP surface narrow and purposeful; Telegraph exposes notification retrieval and mark-read behavior while the queue side handles user-facing delivery work.
- Test with real infrastructure by combining a shared test server, real queue transport, and an SMTP container or equivalent delivery probe.

## Extending the Flow

- When adding a new Telegraph event type, start by deciding whether it fits the existing queue-first delivery model or whether it would justify a separate synchronous API surface; most user-facing delivery work belongs on the queue path.
- Add the event to the relevant `queues.*` config entry, choose the `modes` it should trigger, and keep that event name aligned with the publisher, descriptor filename, and test payload.
- Create or update the descriptor and templates together: email paths need a text template and can optionally add HTML, while notification paths need body text plus a stable title story.
- Reuse the existing email and notification processors when the new event is only content variation; introduce new processor wiring only when you need a genuinely new mode or side effect.
- Close the loop with an end-to-end test that publishes the real event payload through SQS and waits for the concrete side effect the user would care about, such as SMTP output or persisted notification rows.

## Open Questions

- If the service plans to support channels beyond email and in-app notifications, define that channel model in config, domain types, processors, and tests together rather than leaving some layers ahead of others. ^[ambiguous]

## Sources

- [[projects/telegraph/telegraph]] - Service where this pattern is applied concretely.
- [[projects/telegraph/concepts/queue-driven-command-processing]] - Async command path behind the queue consumer.
- [[projects/telegraph/concepts/descriptor-driven-communications]] - Descriptor and template layer that builds the actual messages.
- [[projects/telegraph/references/telegraph-event-processing]] - End-to-end event path that turns queued events into rendered output.
- [[projects/telegraph/references/telegraph-testing-and-smtp-fixtures]] - Test harness that validates the pattern end to end.
- [[skills/building-rustycog-services]] - Broader service-construction workflow for the shared SDK Telegraph builds on.
