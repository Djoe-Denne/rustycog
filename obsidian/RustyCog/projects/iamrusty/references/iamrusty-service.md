---
title: IAMRusty Service
category: references
tags: [reference, iam, architecture, visibility/internal]
sources:
  - IAMRusty/README.md
  - IAMRusty/docs/ARCHITECTURE.md
  - IAMRusty/domain/src/entity/events.rs
  - IAMRusty/setup/src/app.rs
  - IAMRusty/http/src/lib.rs
summary: IAMRusty-specific runtime notes layered on top of the shared RustyCog service shell, emphasizing route inventory, security wiring, and the iam-events versus transport split.
provenance:
  extracted: 0.81
  inferred: 0.13
  ambiguous: 0.06
created: 2026-04-14T17:46:37.6929647Z
updated: 2026-04-19T12:08:26.9393504Z
---

# IAMRusty Service

This page is the IAMRusty-specific companion to `[[projects/rustycog/references/index]]` and `[[references/rustycog-service-construction]]`. It keeps the parts of `[[projects/iamrusty/iamrusty]]` that differ meaningfully from the generic RustyCog service story.

## RustyCog Baseline

- `[[projects/rustycog/references/index]]` is the shared reference map for the service layout, command runtime, config loading, HTTP shell, queue transport, and testing harness this page assumes.
- `[[references/rustycog-service-construction]]` and `[[skills/building-rustycog-services]]` cover the generic composition-root story that IAMRusty reuses.
- `[[projects/rustycog/references/rustycog-command]]`, `[[projects/rustycog/references/rustycog-config]]`, `[[projects/rustycog/references/rustycog-http]]`, and `[[projects/rustycog/references/rustycog-events]]` explain the shared primitives that are not repeated here.

## Service-Specific Differences

- The service is split across domain, application, infrastructure, HTTP, configuration, setup, and migration crates, but its composition root is specialized around auth, token, provider, and queue-backed event services rather than generic CRUD flows.
- `setup/src/app.rs` is the key runtime assembly point, creating database pools, combined repositories, JWT and registration-token services, password adapters, queue-backed event publishing, use cases, and the final `GenericCommandService`.
- The HTTP route table includes public signup, login, verification, resend-verification, registration completion, password reset, OAuth login, callback, token refresh, and JWKS endpoints, plus authenticated profile, provider-token, link, relink, and authenticated reset behavior.
- The runtime builds separate OAuth and token-repository instances for login, provider linking, and internal provider-token operations, which keeps those flows isolated while still sharing domain abstractions.
- Event publishing is part of the service composition, not an afterthought: `create_multi_queue_event_publisher` is wired into auth, registration, and password-reset flows through `IAMErrorMapper`.
- IAMRusty separates event concerns cleanly: `iam-events` defines domain-event contracts, while `[[projects/rustycog/references/rustycog-events]]` provides transport adapters and publisher runtime behavior.
- The high-level docs and the current route table do not match perfectly; some documentation still describes `/start`-style endpoints and older callback assumptions that differ from the live `http/src/lib.rs` surface. ^[ambiguous]

## Open Questions

- `migration/` is part of the wider runtime picture, but it was only indirectly inspected in this ingest batch. ^[ambiguous]
- `README.md` still references a missing `docs/TEST_DATABASE_GUIDE.md`, so the published service docs are incomplete relative to the repo. ^[ambiguous]

## Sources

- [[projects/iamrusty/iamrusty]] - Main project overview.
- [[projects/iamrusty/concepts/hexagonal-architecture]] - Structural pattern behind the crate split.
- [[projects/iamrusty/references/iamrusty-api-and-auth-flows]] - Route-level behavior and auth contracts.
- [[projects/iamrusty/references/iamrusty-runtime-and-security]] - Runtime config, JWT, TLS, and queue context.
- [[projects/rustycog/references/index]] - RustyCog crate map for shared runtime dependencies.
