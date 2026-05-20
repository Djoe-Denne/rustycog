---
title: >-
  AIForAll
category: project
tags: [platform, microservices, rust, visibility/internal]
sources:
  - README.md
  - Cargo.toml
  - monolith/Cargo.toml
  - monolith/src/runtime.rs
summary: >-
  Repo-level map of the AIForAll platform covering core services, shared infrastructure, and the two runtime modes: microservices and modular monolith.
provenance:
  extracted: 0.82
  inferred: 0.16
  ambiguous: 0.00
created: 2026-04-14T16:54:59.5971424Z
updated: 2026-04-25T11:42:00Z
---

# AIForAll

AIForAll is a Rust-based microservices workspace centered on `<!-- [[projects/iamrusty/iamrusty]] -->`, `<!-- [[projects/telegraph/telegraph]] -->`, shared libraries in `<!-- [[projects/rustycog/rustycog]] -->`, and event contracts such as `<!-- [[projects/hive-events/hive-events]] -->`.

## Key Ideas

- The workspace is organized as a `<!-- [[concepts/event-driven-microservice-platform]] -->` with clear service boundaries and shared local infrastructure.
- A top-level Docker Compose flow runs IAMRusty and Telegraph alongside PostgreSQL and LocalStack for local development.
- The Rust services now have two runtime modes: standalone microservice binaries and the `oodhive-monolith` modular monolith that serves nested service routers under one HTTP listener.
- Shared patterns and building blocks are factored into `<!-- [[concepts/shared-rust-microservice-sdk]] -->`, which reduces duplication across services. ^[inferred]
- The broader project-service direction described in `<!-- [[projects/manifesto/manifesto]] -->` extends the same platform model beyond identity and messaging.

## Runtime Modes

- **Microservices:** `iam-service`, `telegraph-service`, `hive-service`, and `manifesto-service` remain independently runnable packages.
- **Modular monolith:** `[[projects/aiforall/references/modular-monolith-runtime]]` documents the `oodhive-monolith` package, which composes IAMRusty, Telegraph, Hive, and Manifesto routers at `/iam`, `/telegraph`, `/hive`, and `/manifesto` while keeping SQS/event semantics unchanged.

## Roadmap

- [[projects/aiforall/roadmap]] captures the near-term platform focus: Sentinel Sync service tests, transactional DB load verification, and the RustyCog Events outbox pattern.

## Open Questions

- The top-level README names `iam-events`, but this source batch does not explain how it differs from `<!-- [[projects/hive-events/hive-events]] -->`.
- Telegraph is described as handling SMS as well as email and notifications, but the live docs still describe email and notification flows more concretely than SMS delivery. ^[ambiguous]

## Sources

- <!-- [[references/aiforall-platform]] --> — Repository overview and shared dev workflow
- [[projects/aiforall/references/modular-monolith-runtime]] — Runtime-mode decision and monolith composition notes
