---
title: >-
  Manifesto Service and Project ADR
category: references
tags: [reference, projects, components, visibility/internal]
sources:
  - Manifesto/README.md
  - Manifesto/SETUP.md
  - Manifesto/IMPLEMENTATION_STATUS.md
  - Manifesto/src/main.rs
  - Manifesto/setup/src/app.rs
  - Manifesto/http/src/lib.rs
  - Manifesto/application/src/command/factory.rs
  - Manifesto/configuration/src/lib.rs
  - Manifesto/tests/common.rs
  - Manifesto/docs/rustycog-implementation-and-usage-guide.md
summary: >-
  Manifesto-specific runtime notes that sit on top of the shared RustyCog service shell,
  highlighting project-domain wiring and the current live-runtime boundary.
provenance:
  extracted: 0.88
  inferred: 0.08
  ambiguous: 0.04
created: 2026-04-14T16:54:59.5971424Z
updated: 2026-04-19T18:00:00Z
---

# Manifesto Service and Project ADR

This page is the Manifesto-specific companion to `[[projects/rustycog/references/index]]` and `[[references/rustycog-service-construction]]`. It assumes the shared RustyCog service shell is already understood and keeps the details that are unique to `[[projects/manifesto/manifesto]]`.

## RustyCog Baseline

- `[[projects/rustycog/references/index]]` is the canonical map for the shared crates and runtime conventions this service composes.
- `[[references/rustycog-service-construction]]` and `[[skills/building-rustycog-services]]` cover the generic order of operations: typed config, composition root, command registry, `AppState`, `RouteBuilder`, and integration tests.
- `[[projects/rustycog/references/rustycog-command]]`, `[[projects/rustycog/references/rustycog-config]]`, `[[projects/rustycog/references/rustycog-http]]`, and `[[projects/rustycog/references/rustycog-testing]]` explain the shared behavior that this page does not repeat.

## Service-Specific Differences

- Manifesto owns project records, component attachments, and member access with explicit lifecycle and state models that do not exist in the framework itself.
- `src/main.rs` follows the standard RustyCog boot path, but the live service uses `ManifestoConfig`, `ManifestoCommandRegistryFactory`, and a composition root specialized around project orchestration.
- `setup/src/app.rs` adds a multi-queue event publisher, a component-service client, an optional apparatus consumer, and resource-scoped permission fetchers for `project`, `component`, and `member` resources before handing everything to `GenericCommandService`.
- `http/src/lib.rs` exposes project, component, member, and permission-management routes, so the service's HTTP surface is shaped by Manifesto's domain more than by generic RustyCog routing concerns.
- `tests/common.rs` keeps the shared real-server harness but fixes the default Manifesto posture at `has_sqs() == false`, making DB-backed integration tests and focused unit tests the primary confidence path.
- The main runtime knobs advertised by the repo docs are now genuinely wired: verified auth secret, logging level, command retry, component-service timeout/api key, and business limits.
- Checked-in local/test configs disable queues deliberately, even though the runtime can publish and consume queue-backed events when explicitly configured.

## Open Questions

- Should the checked-in examples eventually include a non-disabled queue profile for staging-style documentation, or stay conservative for local/test defaults?
- When component provisioning grows beyond type validation, which parts belong in Manifesto versus the external component runtime?

## Sources

- [[projects/manifesto/manifesto]] — Service overview page
- [[projects/rustycog/references/index]] — Shared framework baseline for the runtime being specialized here
- [[projects/manifesto/concepts/component-based-project-orchestration]] — Main architectural concept extracted here
- [[references/rustycog-service-construction]] — Manifesto-authored RustyCog guidance checked against the live service
- [[concepts/event-driven-microservice-platform]] — Async coordination pattern tied to project/component change
