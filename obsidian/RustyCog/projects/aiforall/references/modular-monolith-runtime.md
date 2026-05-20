---
title: >-
  Modular Monolith Runtime
category: references
tags: [reference, platform, architecture, rust, visibility/internal]
sources:
  - Cargo.toml
  - README.md
  - monolith/Cargo.toml
  - monolith/src/runtime.rs
  - monolith/src/routes.rs
  - rustycog/rustycog-http/src/builder.rs
  - IAMRusty/http/src/lib.rs
  - Telegraph/setup/src/app.rs
  - Hive/setup/src/app.rs
  - Manifesto/setup/src/app.rs
summary: >-
  AIForAll now supports standalone microservices and a modular monolith named oodhive-monolith, with both modes using the same bounded-context route prefixes.
provenance:
  extracted: 0.82
  inferred: 0.15
  ambiguous: 0.03
created: 2026-04-25T10:04:00Z
updated: 2026-04-25T10:07:00Z
---

# Modular Monolith Runtime

AIForAll now has two supported runtime shapes:

- Standalone microservices remain available through packages such as `iam-service`, `telegraph-service`, `hive-service`, and `manifesto-service`, and each standalone route surface is mounted under its bounded-context prefix.
- The modular monolith is a separate workspace package currently named `oodhive-monolith`, built from the `monolith/` crate.

## Monolith Composition

The monolith is a composition root, not a service-internals merge. It loads the existing service configs, builds each service through its setup crate, extracts each service router, and serves one nested Axum router through `rustycog_http::serve_router`.

Routes are nested by bounded context:

- `/iam/...`
- `/telegraph/...`
- `/hive/...`
- `/manifesto/...`
- `/health`

Service health routes stay under their nested routers: `/iam/health`, `/telegraph/health`, `/hive/health`, and `/manifesto/health`.

Standalone service binaries intentionally use the same path prefixes: IAMRusty under `/iam`, Telegraph under `/telegraph`, Hive under `/hive`, and Manifesto under `/manifesto`. This keeps API clients from changing URLs when switching between microservice and monolith deployment modes.

## Background Work

The monolith does not call service `run()` methods. It starts only the background tasks that are part of the service runtime:

- Telegraph starts its SQS/event consumer through `TelegraphApp::start_background_tasks()`.
- Manifesto starts its optional apparatus consumer when configured.
- IAM and Hive publish events through their existing queue-backed publishers but do not start a consumer loop in this runtime.

This preserves the platform's `[[concepts/event-driven-microservice-platform]]` behavior for v1: SQS and existing RustyCog event factories remain the coordination mechanism.

## RustyCog HTTP Change

`[[projects/rustycog/references/rustycog-http]]` was split so `RouteBuilder` can produce a router without binding a port. The compatibility path remains:

- `RouteBuilder::into_router()` finalizes routes, middleware, panic handling, correlation propagation, tracing, and state.
- `rustycog_http::serve_router(router, ServerConfig)` owns HTTP/TLS binding.
- `RouteBuilder::build(config)` delegates to the new split for existing standalone services.

This split is the key enabler for one process that keeps service-specific `AppState` values while mounting routers under one listener. ^[inferred]

## Related

- [[projects/aiforall/aiforall]]
- [[projects/aiforall/skills/running-aiforall-runtime-modes]]
- [[references/aiforall-platform]]
- [[projects/rustycog/references/rustycog-http]]
- [[concepts/shared-rust-microservice-sdk]]
- [[concepts/event-driven-microservice-platform]]
