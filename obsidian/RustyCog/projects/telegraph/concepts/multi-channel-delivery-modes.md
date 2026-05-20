---
title: Multi-Channel Delivery Modes
category: concepts
tags: [communication, notifications, sms, visibility/internal]
sources:
  - Telegraph/config/default.toml
  - Telegraph/config/development.toml
  - Telegraph/configuration/src/lib.rs
  - Telegraph/domain/src/entity/communication.rs
  - Telegraph/infra/src/event/processors/mod.rs
  - Telegraph/http/src/handlers/communication.rs
  - Telegraph/migration/src/m20250201_000001_create_notification_tables.rs
summary: Telegraph’s config and storage model describe multiple delivery channels, but the active runtime currently wires email and in-app notifications more fully than SMS.
provenance:
  extracted: 0.74
  inferred: 0.14
  ambiguous: 0.12
created: 2026-04-14T18:18:24.0602572Z
updated: 2026-04-14T18:18:24.0602572Z
---

# Multi-Channel Delivery Modes

`[[projects/telegraph/telegraph]]` is designed as a communication service, not just a notification table with an email sidecar. Its config files, delivery schema, and some unused HTTP DTOs all point toward multiple delivery channels, but the live runtime wires only part of that broader model today.

## Key Ideas

- Telegraph's per-event config uses `modes = [...]` lists under `queues.*.<event>` to decide which handlers should run for a given domain event, so channel choice is data-driven at the queue-routing layer.
- The active processor composite registers only `"email"` and `"notification"` handlers, which means those are the only delivery modes currently reachable through the configured event pipeline.
- `CommunicationMode` and `Communication` in the domain model currently encode only `Email` and `Notification`, so the type system is narrower than the broader channel story described elsewhere.
- `http/src/handlers/communication.rs` defines request DTOs for email, notification, and SMS payloads, which suggests a planned direct-send API surface even though those handlers are not registered in the live route table. ^[ambiguous]
- The notification delivery migration reserves delivery-method strings such as `email`, `sms`, `push`, and `in_app`, so the persistence model anticipates more channels than the current processors actively create.
- `config/default.toml` explicitly documents `[communication.sms]`, while `CommunicationConfig` currently includes `email`, `notification`, and `template` sections only. Conflict to resolve. ^[ambiguous]

## Open Questions

- Conflict to resolve: SMS appears in config, migration comments, and unused HTTP DTOs, but not in the live `CommunicationMode` enum or `CompositeEventProcessor::with_all_processors()` wiring. ^[ambiguous]
- The migration uses delivery-method vocabulary like `push` and `in_app`, while the runtime mostly talks about `notification`; it is not yet clear whether those are synonyms or separate future modes. ^[ambiguous]

## Sources

- [[projects/telegraph/telegraph]] - Service where these mode differences matter operationally.
- [[projects/telegraph/concepts/descriptor-driven-communications]] - Descriptor system that feeds the active email and notification channels.
- [[projects/telegraph/references/telegraph-event-processing]] - Event pipeline that currently wires the active handlers.
- [[projects/telegraph/references/telegraph-http-and-notification-api]] - Live route surface versus broader communication DTOs.
