---
title: >-
  Hive Events
category: project
tags: [events, sqs, integration, visibility/internal]
sources:
  - hive-events/README.md
summary: >-
  Hive Events is a shared crate of organization-domain event contracts and queue names used for inter-service communication between Hive and downstream consumers.
provenance:
  extracted: 0.88
  inferred: 0.12
  ambiguous: 0.00
created: 2026-04-14T16:54:59.5971424Z
updated: 2026-04-14T18:56:22.3888182Z
---

# Hive Events

Hive Events is a contract crate for domain events emitted by the Hive organization domain. It strengthens the repo's `<!-- [[concepts/event-driven-microservice-platform]] -->` by defining payloads and queue-routing conventions that downstream services, including `<!-- [[projects/telegraph/telegraph]] -->`, can consume.

## Key Ideas

- The crate groups events around organization lifecycle, members, invitations, and external integrations.
- Queue routing is explicit, with separate queues for organization state, notifications, and sync monitoring.
- The crate gives the platform a typed event surface even when the producer service itself is not co-located in this repository. ^[inferred]
- Its notification queue integration complements the higher-level platform picture in `<!-- [[projects/aiforall/aiforall]] -->`.
- The producer service is now documented directly at `<!-- [[projects/hive/hive]] -->`, so this page can stay focused on the contract crate rather than trying to explain the whole runtime. ^[inferred]

## Open Questions

- The relationship between Hive project/org events and `<!-- [[projects/manifesto/manifesto]] -->` membership cascading is architectural rather than implementation-level in the current docs.
- The crate explains queue categories well, but it still does not map every event type to a specific consuming workflow in the rest of the platform. ^[ambiguous]

## Sources

- <!-- [[references/platform-building-blocks]] --> — Shared SDK and event-contract building blocks
- <!-- [[projects/hive/hive]] --> — Producer service that emits these events
