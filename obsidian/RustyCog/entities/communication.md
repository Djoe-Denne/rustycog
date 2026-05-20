---
title: Communication
category: entities
tags: [communication, notifications, templates, visibility/internal]
sources:
  - Telegraph/domain/src/entity/communication.rs
  - Telegraph/domain/src/entity/delivery.rs
  - Telegraph/domain/src/entity/template.rs
summary: Telegraph models communication as a user-facing message domain with communication payloads, templates, and delivery records.
provenance:
  extracted: 0.84
  inferred: 0.08
  ambiguous: 0.08
created: 2026-04-14T20:28:20.9129598Z
updated: 2026-04-14T20:28:20.9129598Z
---

# Communication

The canonical communication entity family lives in `<!-- [[projects/telegraph/telegraph]] -->`. Telegraph does not model only one “notification” row; it models communication payloads, reusable templates, and delivery records as separate but connected entities.

## Key Ideas

- `Communication` is the high-level domain shape and currently branches into `Email` and `Notification` variants around a shared recipient structure.
- `MessageTemplate` stores reusable content plus per-mode rendering logic so the same event can produce standardized output.
- `MessageDelivery` tracks provider-facing execution state such as attempts, sent/delivered/failed status, metadata, and provider message IDs.
- `DeliveryAttempt` captures attempt-by-attempt telemetry beneath one delivery record.
- Telegraph therefore treats “message body”, “template”, and “delivery outcome” as distinct entities rather than one overloaded notification table.

## Open Questions

- The domain entity set still includes SMS-oriented template content even though the currently wired runtime emphasizes email and notification flows more strongly. ^[ambiguous]
- The wiki still does not clearly separate what is persisted as a user-visible notification versus what remains a delivery/runtime record. ^[ambiguous]

## Sources

- <!-- [[projects/telegraph/references/telegraph-entity-model]] --> - Telegraph's full entity inventory.
- <!-- [[projects/telegraph/concepts/descriptor-driven-communications]] --> - Descriptor system that produces these entities.
- <!-- [[projects/telegraph/references/telegraph-event-processing]] --> - Runtime path that fills communications and deliveries from events.
- <!-- [[projects/telegraph/references/telegraph-http-and-notification-api]] --> - API surface that exposes the notification side of the model.
