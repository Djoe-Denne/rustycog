---
title: Telegraph Entity Model
category: references
tags: [reference, entities, communication, visibility/internal]
sources:
  - Telegraph/domain/src/entity/communication.rs
  - Telegraph/domain/src/entity/delivery.rs
  - Telegraph/domain/src/entity/template.rs
summary: Inventory of Telegraph's communication, template, and delivery entities, including the split between user-visible notifications and provider-facing delivery records.
provenance:
  extracted: 0.87
  inferred: 0.07
  ambiguous: 0.06
created: 2026-04-14T20:28:20.9129598Z
updated: 2026-04-19T11:38:52.5746779Z
---

# Telegraph Entity Model

This page lists the main entities `[[projects/telegraph/telegraph]]` owns in its communication domain.

## Key Entities

- `Communication` is the high-level message entity and currently branches into email and notification variants.
- `CommunicationRecipient` is embedded in those payloads so a message can target a user ID, an email address, or both.
- `CommunicationDescriptor` and its per-mode subtypes load descriptor metadata that steers how user-facing messages are built.
- `MessageTemplate` stores reusable content plus placeholder rendering logic.
- `NotificationCommunication` is the user-visible read model exposed by Telegraph's HTTP API, while `MessageDelivery` and `DeliveryAttempt` track provider-facing execution details and retries behind the scenes.
- `MessageDelivery` and `DeliveryAttempt` track provider-facing execution, retries, outcomes, and delivery telemetry.

## Open Questions

- The entity layer still carries SMS-oriented template content, but the live runtime and wiki emphasize email and notification flows more heavily. ^[ambiguous]

## Sources

- [[entities/communication]] - Canonical communication-side entity page.
- [[projects/telegraph/references/telegraph-event-processing]] - Runtime path that fills and sends these entities.
- [[projects/telegraph/references/telegraph-http-and-notification-api]] - API surface exposing the notification part of the model.
- [[projects/telegraph/concepts/descriptor-driven-communications]] - Descriptor pattern that shapes the communication entities.
