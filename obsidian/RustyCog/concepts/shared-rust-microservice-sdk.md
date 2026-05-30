---
title: >-
  Shared Rust Microservice SDK
category: concepts
tags: [sdk, rust, platform, visibility/internal]
sources:
  - rustycog/README.md
  - Cargo.toml
  - rustycog/Cargo.toml
  - rustycog/rustycog-core/src/error.rs
  - rustycog/rustycog-command/src/lib.rs
  - rustycog/rustycog-command/src/registry.rs
  - rustycog/rustycog-config/src/lib.rs
  - rustycog/rustycog-db/src/lib.rs
  - rustycog/rustycog-events/src/lib.rs
  - rustycog/rustycog-http/src/builder.rs
  - rustycog/rustycog-permission/src/lib.rs
  - rustycog/rustycog-logger/src/lib.rs
  - rustycog/rustycog-testing/src/lib.rs
  - Manifesto/docs/rustycog-service-build-guide.md
  - Manifesto/docs/rustycog-implementation-and-usage-guide.md
summary: >-
  RustyCog is the shared SDK stack for platform services, centered on one feature-gated runtime package (`rustycog-framework`, usually aliased as `rustycog`) with integrated testing helpers behind the `testing` feature.
provenance:
  extracted: 0.78
  inferred: 0.08
  ambiguous: 0.14
created: 2026-04-14T16:54:59.5971424Z
updated: 2026-05-20T14:07:00Z
---

# Shared Rust Microservice SDK

`[[projects/rustycog/rustycog]]` is the shared SDK layer for service runtime concerns in AIForAll. This page captures the architectural idea; feature/module-level details live in `[[projects/rustycog/references/index]]`.

## Key Ideas

- The SDK is split by concern (errors, command runtime, config, DB, events, HTTP shell, permissions, logging, tests) so services compose only needed features/modules from the unified package without redefining runtime primitives.
- RustyCog standardizes composition seams, not business logic: services still own domain models, handlers, route sets, and policy choices. Authorization itself is centralized in OpenFGA via [[concepts/centralized-authorization-service]] — services only call `Check`.
- Shared entities (`ServiceError`, `CommandRegistry`, `QueueConfig`, `RouteBuilder`, `PermissionChecker`, `DomainEvent`, and others) are documented in `[[entities/index]]` as a common vocabulary for service docs.
- Cross-service consistency comes from repeating the same integration boundaries (config -> command -> HTTP -> permissions/events/testing), not from one monolithic starter template.
- Consumption is now explicit: runtime crates depend on `rustycog = { package = "rustycog-framework", ... }` with selected features, and integration suites add the `testing` feature when they need fixtures.

## Open Questions

- The wiki still lacks a service-by-service adoption matrix for the active RustyCog feature sets.
- Stable vs evolving RustyCog surfaces are not yet marked explicitly for consumers.
- Some runtime edges remain ambiguous, especially the current health-only scope behind the historical `rustycog-server` reference. ^[ambiguous]

## Sources

- [[projects/rustycog/rustycog]] — SDK hub and ownership boundaries
- [[projects/rustycog/references/index]] — Canonical module/feature inventory and detailed behavior
- [[references/rustycog-service-construction]] — Manifesto guide-to-runtime construction analysis
- [[skills/building-rustycog-services]] — Practical service assembly workflow