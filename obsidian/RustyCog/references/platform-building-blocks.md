---
title: >-
  Platform Building Blocks
category: references
tags: [reference, sdk, events, visibility/internal]
sources:
  - rustycog/README.md
  - Cargo.toml
  - rustycog/Cargo.toml
  - rustycog/rustycog-config/src/lib.rs
  - rustycog/rustycog-events/src/lib.rs
  - rustycog/rustycog-http/src/builder.rs
  - rustycog/rustycog-testing/src/common/kafka_testcontainer.rs
  - rustycog/rustycog-testing/src/common/sqs_testcontainer.rs
  - hive-events/README.md
summary: >-
  Source summary for the shared Rust SDK stack (`rustycog-framework` aliased as `rustycog`, including the `testing` feature) and event-contract modules that give services a common runtime, transport, and testing foundation.
provenance:
  extracted: 0.86
  inferred: 0.09
  ambiguous: 0.05
created: 2026-04-14T16:54:59.5971424Z
updated: 2026-05-20T14:07:00Z
---

# Platform Building Blocks

These sources cover the shared platform substrate beneath application services: `[[projects/rustycog/rustycog]]` for runtime mechanics and domain event transport contracts.

## Key Ideas

- RustyCog provides the shared execution shell: command runtime, typed config, DB pooling, HTTP composition, permission hooks, event adapters, logging setup, and integration-test scaffolding.
- Hive Events provides service-agnostic event names and payload contracts for organization-domain flows.
- Together they separate *how services run* (SDK/runtime) from *what services say* (event schemas), which keeps platform concerns reusable.
- Queue transport remains polymorphic (`Kafka`, `SQS`, disabled/no-op), enabling one event API across local and production-like environments.
- This page is intentionally platform-level; module-by-module internals are delegated to `[[projects/rustycog/references/index]]` and related reference pages.

## Open Questions

- The wiki still does not map a precise service-by-service adoption matrix for these building blocks.
- Transport defaults (Kafka vs SQS per service/environment) are still only partially documented. ^[ambiguous]

## Sources

- [[projects/rustycog/rustycog]] — Shared SDK project
- [[projects/rustycog/references/index]] — Canonical RustyCog module/feature inventory
- [[concepts/shared-rust-microservice-sdk]] — Architectural framing of the SDK model
- [[references/rustycog-service-construction]] — Construction-level guidance using this substrate