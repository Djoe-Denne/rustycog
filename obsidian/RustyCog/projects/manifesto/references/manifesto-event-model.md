---
title: Manifesto Event Model
category: references
tags: [reference, events, projects, visibility/internal]
sources:
  - Manifesto/setup/src/app.rs
  - Manifesto/application/src/usecase/project.rs
  - Manifesto/application/src/usecase/component.rs
  - Manifesto/application/src/usecase/member.rs
  - Manifesto/infra/src/event/consumer.rs
  - Manifesto/infra/src/event/processors/component_processor.rs
summary: >-
  Code-backed view of Manifesto's live event behavior: best-effort publication of Manifesto
  domain events plus inbound apparatus component-status consumption when queues are enabled.
provenance:
  extracted: 0.89
  inferred: 0.07
  ambiguous: 0.04
created: 2026-04-14T20:25:00Z
updated: 2026-04-19T18:00:00Z
---

# Manifesto Event Model

`[[projects/manifesto/manifesto]]` has two live event behaviors today: it publishes Manifesto domain events from application flows, and it can consume apparatus component-status events to reconcile stored component state.

## Key Ideas

- Project flows publish `ProjectCreated`, `ProjectUpdated`, `ProjectDeleted`, `ProjectPublished`, and `ProjectArchived`.
- Component flows publish `ComponentAdded`, `ComponentStatusChanged`, and `ComponentRemoved`.
- Member flows publish `MemberAdded`, `MemberPermissionsUpdated`, `MemberRemoved`, `PermissionGranted`, and `PermissionRevoked`.
- `setup/src/app.rs` injects the same `EventPublisher` into project, component, and member use cases, defaulting to a multi-queue publisher unless tests or alternate bootstraps override it.
- Event publication remains best-effort: failures are logged with `tracing::warn!` but do not roll back the main business transaction.
- `ApparatusEventConsumer` is constructed in setup and started alongside the HTTP server only when queue config resolves to a real consumer.
- `ComponentStatusProcessor` handles inbound `apparatus_events::ComponentStatusChangedEvent` messages by updating the matching stored component:
  - duplicates are treated as no-ops,
  - stale events are ignored,
  - applied timestamps use the event's `changed_at`.
- The old unused outbound apparatus adapter is no longer part of the live runtime. Outbound publication currently uses only Manifesto's own domain-event vocabulary.

## Checked-In Queue Posture

- Checked-in `default`, `development`, and `test` configs all disable queues.
- That means local/test boots use no-op publisher/consumer behavior unless queue settings are explicitly overridden.
- Focused runtime tests also cover the enabled-config path falling back to a safe no-op consumer when no broker fixture is present.

## Open Questions

- Should any Manifesto domain events eventually become hard-fail instead of best-effort?
- If queue-backed operation becomes a default CI path later, which event contracts deserve end-to-end broker coverage instead of unit-level runtime tests?

## Sources

- [[projects/manifesto/manifesto]] - Service overview and runtime context.
- [[projects/manifesto/references/manifesto-api-and-permission-flows]] - Route and use-case entrypoints that trigger these events.
- [[projects/manifesto/concepts/project-ownership-and-publication-lifecycle]] - Project lifecycle transitions and their emitted events.
- [[projects/manifesto/concepts/component-catalog-and-fallback-adapter]] - Component-side validation and runtime status updates.
- [[concepts/event-driven-microservice-platform]] - Platform-wide async coordination context.
