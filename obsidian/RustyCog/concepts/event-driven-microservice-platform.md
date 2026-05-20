---
title: >-
  Event-Driven Microservice Platform
category: concepts
tags: [architecture, microservices, events, visibility/internal]
sources:
  - README.md
  - docs/project/Archi.md
  - IAMRusty/README.md
  - Hive/application/src/usecase/organization.rs
  - Hive/application/src/usecase/invitation.rs
  - Hive/application/src/usecase/external_link.rs
  - Hive/application/src/usecase/sync_job.rs
  - rustycog/README.md
  - rustycog/rustycog-events/src/lib.rs
  - rustycog/rustycog-events/src/event.rs
  - rustycog/rustycog-outbox/src/lib.rs
  - rustycog/rustycog-testing/src/common/kafka_testcontainer.rs
  - rustycog/rustycog-testing/src/common/sqs_testcontainer.rs
  - hive-events/README.md
summary: >-
  The platform uses decoupled services plus transport-neutral events, SQS fanout, and a transactional outbox for durable event intent.
provenance:
  extracted: 0.8
  inferred: 0.12
  ambiguous: 0.08
created: 2026-04-14T16:54:59.5971424Z
updated: 2026-04-26T13:36:00Z
---

# Event-Driven Microservice Platform

AIForAll favors asynchronous coordination between bounded services instead of pushing all workflows through synchronous request chains. The clearest examples connect `<!-- [[projects/iamrusty/iamrusty]] -->`, `<!-- [[projects/hive/hive]] -->`, `<!-- [[projects/telegraph/telegraph]] -->`, `<!-- [[projects/hive-events/hive-events]] -->`, and the project-service work captured in `<!-- [[projects/manifesto/manifesto]] -->`.

## Key Ideas

- IAMRusty publishes user-signup style events and Telegraph consumes them to send notifications.
- Hive publishes organization, member, invitation, external-link, and sync-job events through `<!-- [[projects/hive-events/hive-events]] -->`, which broadens the platform story beyond identity flows alone.
- Hive Events routes messages into purpose-specific queues such as `notification-events` and `sync-events`.
- `[[projects/rustycog/rustycog]]` provides shared transport and envelope abstractions; crate-level event mechanics belong to `[[projects/rustycog/references/rustycog-events]]`.
- `QueueConfig` and factory wiring let services switch Kafka/SQS/no-op modes without rewriting higher-level event call sites.
- SQS fanout is now represented in config rather than service-specific code: an event type maps to a list of destination queues, and the RustyCog SQS publisher sends the same event to each queue.
- SQS consumers poll every configured physical queue independently and delegate all accepted messages to the same service handler.
- `[[projects/rustycog/references/rustycog-outbox]]` gives write-heavy services a durable event-intent bridge: domain rows and outbox rows are committed together, then an embedded dispatcher publishes later.
- The test harness shows both transports are active parts of the codebase: Kafka tests provision a KRaft container and consume messages back from the topic, while SQS tests provision LocalStack and exercise real queue URLs and message bodies.
- Hive is a good example of an HTTP-first service that still emits a substantial event stream, while Telegraph is a queue-aware consumer and IAMRusty combines HTTP-first flows with queue-backed side effects. ^[inferred]
- Asynchronous messaging lets services keep ownership over their own data and still participate in longer workflows. ^[inferred]
- The SDK now makes Kafka and SQS both first-class options in code, but the wiki still does not show which services standardize on which transport in production. ^[ambiguous]

## Open Questions

- The boundary between queue-backed domain events and any Kafka-based internal event tooling is not documented end to end.
- The event factories fall back to no-op publishers/consumers when transports are disabled or fail to initialize, so the desired production stance toward that fallback is not yet documented. ^[ambiguous]
- Retry, dead-letter, and observability strategies are only partially described in this ingest pass.

## Sources

- [[references/aiforall-platform]] — Repo-level workflow and service communication
- <!-- [[projects/hive/hive]] --> — Organization-management service that emits Hive domain events
- <!-- [[projects/manifesto/references/manifesto-service]] --> — Project-service orchestration and cascading ADR
- [[projects/rustycog/references/index]] — Code-backed inventory of the event crates
- [[projects/rustycog/references/rustycog-events]] — Crate-level event transport and publisher details
- [[projects/rustycog/references/rustycog-outbox]] — Durable event-intent bridge between DB transactions and event dispatch
- [[projects/rustycog/rustycog]] — Shared SDK project implementing transport abstractions.
- [[entities/domain-event]] — Shared event envelope entity
- [[references/platform-building-blocks]] — Event contracts and shared infrastructure primitives