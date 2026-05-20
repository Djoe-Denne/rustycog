---
title: >-
  RustyCog
category: project
tags: [sdk, rust, platform, visibility/internal]
sources:
  - rustycog/README.md
  - rustycog/Cargo.toml
  - rustycog/src/lib.rs
  - rustycog/rustycog-command/src/registry.rs
  - rustycog/rustycog-config/src/lib.rs
  - rustycog/rustycog-events/src/lib.rs
  - rustycog/rustycog-http/src/builder.rs
  - rustycog/rustycog-permission/src/lib.rs
  - rustycog/rustycog-testing/src/common/test_server.rs
  - Manifesto/docs/rustycog-service-build-guide.md
  - Manifesto/docs/rustycog-implementation-and-usage-guide.md
summary: >-
  RustyCog now ships as one feature-gated crate (`rustycog`) plus a separate `rustycog-testing` crate, while preserving module-level runtime building blocks.
provenance:
  extracted: 0.79
  inferred: 0.09
  ambiguous: 0.12
created: 2026-04-14T16:54:59.5971424Z
updated: 2026-05-20T13:55:00Z
---

# RustyCog

RustyCog is the shared Rust framework used to compose service runtime concerns across AIForAll. This page is the orientation hub; module-level details (inside the unified crate) live in `[[projects/rustycog/references/index]]`.

## Documentation Note

- Treat `rustycog/README.md` as context, not the canonical API map; source-of-truth behavior is maintained in `[[projects/rustycog/references/index]]` and linked module reference pages.

## Canonical Scope

RustyCog now exposes one crate (`rustycog`) with feature-gated module surfaces, plus one separate testing crate:

- [[projects/rustycog/references/rustycog-core]] — `rustycog::core` (`core` feature), shared error contracts (`ServiceError`, `DomainError`)
- [[projects/rustycog/references/rustycog-command]] — `rustycog::command` (`command` feature), command execution runtime and registry
- [[projects/rustycog/references/rustycog-config]] — `rustycog::config` (`config` feature), typed config models and loaders
- [[projects/rustycog/references/rustycog-db]] — `rustycog::db` (`db` feature), DB pool and replica-aware read/write routing
- [[projects/rustycog/references/rustycog-events]] — `rustycog::events` (`events` feature), event envelope plus Kafka/SQS/no-op adapters
- [[projects/rustycog/references/rustycog-outbox]] — `rustycog::outbox` (`outbox` feature), transactional outbox recorder and dispatcher
- [[projects/rustycog/references/rustycog-http]] — `rustycog::http` (`http` feature), Axum shell and middleware
- [[projects/rustycog/references/rustycog-permission]] — `rustycog::permission` (`permission` feature), `PermissionChecker` and OpenFGA adapters
- [[projects/rustycog/references/rustycog-server]] — `rustycog::server` (`server` feature), health-check abstractions
- [[projects/rustycog/references/rustycog-logger]] — `rustycog::logger` (`logger` feature), tracing/logging initialization helpers
- [[projects/rustycog/references/rustycog-testing]] — separate `rustycog-testing` crate for integration fixtures and bootstrap helpers
- [[projects/rustycog/references/rustycog-meta]] — legacy packaging note retained for migration history

## Packaging Decision

- Runtime/service crates should depend on `rustycog` and select explicit features (`core`, `config`, `http`, `events`, etc., or `full` when needed).
- Integration tests should continue to use `rustycog-testing`, which depends on `rustycog` with `full` and `test-utils`.
- Historical `rustycog-*` per-crate dependencies are deprecated in this repository.

## Recent Runtime Decisions

- SQS fanout now belongs to the shared `[[projects/rustycog/references/rustycog-events]]` and `[[projects/rustycog/references/rustycog-config]]` crates: services declare per-event destination queue lists, and RustyCog handles publishing the same event to each queue.
- SQS consumers now derive their polling pool from configured physical queues, which lets a service independently consume each queue while sharing one handler.
- `[[projects/rustycog/references/rustycog-outbox]]` now owns the DB-backed event intent table and embedded dispatcher, so services can commit domain rows and event intent atomically without coupling `rustycog-events` to the database.

## Documentation Ownership

- Per-crate API and behavior details: `[[projects/rustycog/references/index]]`
- Shared SDK vocabulary: `[[entities/index]]`
- Cross-crate architecture patterns: `[[concepts/shared-rust-microservice-sdk]]`
- Service-construction usage flow: `[[skills/building-rustycog-services]]`

## Scope Mismatches To Track

- README mentions `rustycog-macros` and examples that are not visible in the checked-in tree. ^[ambiguous]
- `rustycog-server` name suggests broader server bootstrap ownership, but current surface is health-only. ^[ambiguous]

## Sources

- [[projects/rustycog/references/index]] — Inventory and scope boundaries for all runtime modules/features
- [[references/platform-building-blocks]] — Shared SDK plus event-contract context
- [[concepts/shared-rust-microservice-sdk]] — Cross-project framing for the same stack
- [[skills/building-rustycog-services]] — Service composition workflow using these crates