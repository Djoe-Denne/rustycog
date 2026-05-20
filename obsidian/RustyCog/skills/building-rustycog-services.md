---
title: >-
  Building RustyCog Services
category: skills
tags: [rustycog, scaffolding, services, visibility/internal]
sources:
  - Manifesto/docs/rustycog-service-build-guide.md
  - Manifesto/docs/rustycog-hexagonal-web-service-guide.md
  - Manifesto/docs/rustycog-implementation-and-usage-guide.md
  - Manifesto/src/main.rs
  - Manifesto/configuration/src/lib.rs
  - Manifesto/setup/src/app.rs
  - Manifesto/http/src/lib.rs
  - rustycog/rustycog-command/src/registry.rs
  - rustycog/rustycog-config/src/lib.rs
  - rustycog/rustycog-http/src/builder.rs
  - rustycog/rustycog-http/src/middleware_permission.rs
  - rustycog/rustycog-logger/src/lib.rs
  - rustycog/src/lib.rs
  - rustycog/rustycog-testing/src/common/test_server.rs
  - IAMRusty/http/src/lib.rs
  - Telegraph/http/src/lib.rs
  - Hive/http/src/lib.rs
  - Manifesto/http/src/lib.rs
  - monolith/src/routes.rs
  - monolith/src/runtime.rs
  - Cargo.toml
  - rustycog/Cargo.toml
summary: >-
  Workflow for scaffolding RustyCog services on the unified `rustycog` crate (feature-gated modules), plus `rustycog-testing` for integration tests.
provenance:
  extracted: 0.82
  inferred: 0.08
  ambiguous: 0.10
created: 2026-04-14T17:03:47.5107188Z
updated: 2026-05-20T14:04:00Z
---

# Building RustyCog Services

Use this page when starting a new service that should look like `<!-- [[projects/manifesto/manifesto]] -->` and build on `<!-- [[projects/rustycog/rustycog]] -->`.

## Workflow

- Start with one vertical slice across `domain`, `application`, `infra`, `http`, `setup`, `configuration`, and `tests` rather than scaffolding everything at once.
- Depend on `rustycog` and enable only the features you need (`core`, `config`, `http`, `events`, etc., or `full` for broad usage). Keep `rustycog-testing` in dev/test dependencies.
- Define typed config first using the `<!-- [[concepts/structured-service-configuration]] -->` pattern, then decide explicitly whether your service will use `setup_logging` or a hand-rolled tracing initialization; `<!-- [[projects/manifesto/manifesto]] -->` still uses the latter. Conflict to resolve. ^[ambiguous]
- Create one `DbConnectionPool`, split read and write repositories correctly, and wire concrete dependencies inside the setup composition root.
- Register commands through the `<!-- [[concepts/command-registry-and-retry-policies]] -->` approach, then wrap the registry in `GenericCommandService` so handlers stay behind one execution surface.
- Build the centralized `Arc<dyn PermissionChecker>` (`OpenFgaPermissionChecker` wrapped in `CachedPermissionChecker` and `MetricsPermissionChecker`) and pass it into `AppState::new(command_service, user_id_extractor, checker)`. Use `RouteBuilder` so tracing, panic handling, correlation IDs, and the `/health` endpoint stay standardized.
- In the HTTP crate, split reusable route construction from serving: expose `create_router(state) -> axum::Router` for embedding, `SERVICE_PREFIX` for the bounded-context path, and `create_prefixed_router(state)` for standalone microservice mode.
- Keep `create_app_routes(state, server_config)` as the standalone entrypoint, but have it call `rustycog_http::serve_router(create_prefixed_router(state), server_config)` rather than binding an unprefixed router.
- In the setup crate, expose an application-level `router()` method that delegates to the HTTP crate's unprefixed `create_router`. If the service owns background consumers, expose `start_background_tasks()` and `stop_background_tasks()` so `[[projects/aiforall/references/modular-monolith-runtime]]` can compose the service without calling its `run()` method.
- For protected routes call `.with_permission_on(Permission::X, "<openfga_type>")` immediately after `.authenticated()` or `.might_be_authenticated()`. There is no per-route fetcher and no `permissions_dir` chain — `object_type` must match a type defined in [`openfga/model.fga`](../../openfga/model.fga).
- If you load one config subsection directly, remember that `load_config_part("server")` reads `SERVER_*`-prefixed overrides rather than your service prefix. Conflict to resolve. ^[ambiguous]
- Finish the slice with integration tests that exercise auth, permissions, validation, and the happy path, then add Kafka or LocalStack-backed checks only when transport behavior is part of the contract.

## Common Pitfalls

- Letting `command_type()` strings drift away from registration keys.
- Mixing `AuthUser` and `OptionalAuthUser` with the wrong route mode.
- Assuming `config/default.toml` is always merged automatically.
- Defining `[command.retry]`, `logging.level`, or service timeout knobs in TOML without verifying the current runtime path actually consumes them.
- Depending on historical `rustycog-*` per-crate manifests instead of the unified `rustycog` feature set.
- Calling another service's `run()` method from a modular monolith. Compose via setup/build APIs, extract routers, start only background tasks, and serve exactly one top-level router.
- Letting standalone microservice paths drift from monolith paths. The same `SERVICE_PREFIX` constants should define both modes, e.g. `/iam`, `/telegraph`, `/hive`, and `/manifesto`.
- Forgetting that the permission middleware only binds the deepest UUID-shaped path segment into `ResourceRef`. Non-UUID segments (`{component_type}`, `{resource}`) are skipped.
- Emitting a domain event that has no matching translator arm in [[projects/sentinel-sync/sentinel-sync]] — the OpenFGA store falls out of sync with your aggregate state silently.
- Expecting README-level macros or example projects that are not present in the current tree. ^[ambiguous]

## Sources

- <!-- [[references/rustycog-service-construction]] --> — Combined source summary for this workflow
- <!-- [[projects/rustycog/references/index]] --> — Code-backed inventory of the feature modules this workflow wires together
- [[projects/aiforall/references/modular-monolith-runtime]] — Dual runtime mode that service routers must support
- [[projects/aiforall/skills/running-aiforall-runtime-modes]] — Compile and smoke-test workflow for microservice and monolith modes
- <!-- [[concepts/shared-rust-microservice-sdk]] --> — Broader platform motivation for the approach
- <!-- [[projects/iamrusty/concepts/hexagonal-architecture]] --> — Architectural contract the workflow preserves