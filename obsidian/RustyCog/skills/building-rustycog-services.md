---
title: >-
  Building RustyCog Services
category: skills
tags: [rustycog, scaffolding, services, visibility/internal]
sources:
  - rustycog/rustycog-command/src/registry.rs
  - rustycog/rustycog-config/src/lib.rs
  - rustycog/rustycog-http/src/builder.rs
  - rustycog/rustycog-http/src/middleware_permission.rs
  - rustycog/rustycog-logger/src/lib.rs
  - rustycog/src/lib.rs
  - rustycog/rustycog-testing/src/common/test_server.rs
  - Cargo.toml
  - rustycog/Cargo.toml
  - AIForAll/IAMRusty/domain/src/lib.rs
  - AIForAll/IAMRusty/domain/src/port/mod.rs
  - AIForAll/IAMRusty/application/src/lib.rs
  - AIForAll/IAMRusty/infra/src/lib.rs
  - AIForAll/IAMRusty/http/src/handlers/mod.rs
  - AIForAll/IAMRusty/setup/src/app.rs
summary: >-
  Workflow for scaffolding RustyCog services on the unified `rustycog-framework` package (usually aliased as `rustycog`, with feature-gated modules), including the `testing` feature for integration tests.
provenance:
  extracted: 0.82
  inferred: 0.08
  ambiguous: 0.10
created: 2026-04-14T17:03:47.5107188Z
updated: 2026-06-06T14:30:00Z
---

# Building RustyCog Services

Use this page when starting a new service that builds on `[[projects/rustycog/rustycog]]`.

## Workflow

- Start with one vertical slice across `domain`, `application`, `infra`, `http`, `setup`, `configuration`, and `tests` rather than scaffolding everything at once.
- Depend on `rustycog = { package = "rustycog-framework", ... }` and enable only the features you need (`core`, `config`, `http`, `events`, etc., or `full` for broad usage). Enable `testing` in test-only dependency declarations when you need fixtures.
- Define typed config first using the `<!-- [[concepts/structured-service-configuration]] -->` pattern, then decide explicitly whether your service will use `setup_logging` or a hand-rolled tracing initialization.
- Create one `DbConnectionPool`, split read and write repositories correctly, and wire concrete dependencies inside the setup composition root.
- Register commands through the `<!-- [[concepts/command-registry-and-retry-policies]] -->` approach, then wrap the registry in `GenericCommandService` so handlers stay behind one execution surface.
- Build the centralized `Arc<dyn PermissionChecker>` (`OpenFgaPermissionChecker` wrapped in `CachedPermissionChecker` and `MetricsPermissionChecker`) and pass it into `AppState::new(command_service, user_id_extractor, checker)`. Use `RouteBuilder` so tracing, panic handling, correlation IDs, and the `/health` endpoint stay standardized.
- In the HTTP crate, split reusable route construction from serving: expose `create_router(state) -> axum::Router` for embedding, `SERVICE_PREFIX` for the bounded-context path, and `create_prefixed_router(state)` for standalone microservice mode.
- Keep `create_app_routes(state, server_config)` as the standalone entrypoint, but have it call `rustycog::http::serve_router(create_prefixed_router(state), server_config)` rather than binding an unprefixed router.
- In the setup crate, expose an application-level `router()` method that delegates to the HTTP crate's unprefixed `create_router`. If the service owns background consumers, expose `start_background_tasks()` and `stop_background_tasks()` so an embedding runtime can compose the service without calling its `run()` method.
- For protected routes call `.with_permission_on(Permission::X, "<openfga_type>")` immediately after `.authenticated()` or `.might_be_authenticated()`. There is no per-route fetcher and no `permissions_dir` chain — `object_type` must match a type defined in [`openfga/model.fga`](../../openfga/model.fga).
- If you load one config subsection directly, remember that `load_config_part("server")` reads `SERVER_*`-prefixed overrides rather than your service prefix. Conflict to resolve. ^[ambiguous]
- Finish the slice with integration tests that exercise auth, permissions, validation, and the happy path, then add Kafka or LocalStack-backed checks only when transport behavior is part of the contract.

## Crate-internal structure (gold standard)

Keep every crate's `lib.rs` thin — module declarations plus ergonomic re-exports — and give each concern its own subfolder with a `mod.rs`. The canonical reference implementation of this layout is the **IAMRusty** service (`AIForAll/IAMRusty`); DeRust was restructured to match it.

- **`lib.rs` is thin in every crate**: `pub mod ...;` declarations and re-exports only. No types, handlers, or wiring live at the crate root.
- **`domain` = `entity/` + `service/` + `port/` (+ `error.rs`)**. `entity/` holds data/value types; `service/` holds pure domain algorithms (no I/O); `port/` holds the hexagonal trait boundaries, split into `repository.rs` (I/O-shaped ports: readers, ledgers, sinks, sources) and `service.rs` (compute/transform ports: normalize, analyze, plan, apply, validate). **Ports live in `domain`, never in `application`** — this is what lets `infra` depend only on `domain`.
- **`application` = `command/` + `usecase/` (+ `dto/` only when DTOs exist)**. `command/` is the command-pattern layer; `usecase/` is orchestration above the domain ports. Omit `dto/` when the service has no DTO types, and flag the omission so it reads as deliberate rather than forgotten.
- **`infra` = one subfolder per multi-file concern, flat single-file adapters otherwise**. Group related files behind a `mod.rs` (`analysis/`, `repository/`, `auth/`, `token/`, ...); leave one-file adapters flat (`ledger.rs`, `llm.rs`, `sink.rs`). A transport client owned by a single concern (DeRust's `lsp/`) can sit as its own sibling folder rather than nesting under the concern, to minimize churn.
- **`http` = `handlers/` (one module per resource) + `error.rs` + a feature-state file** (`runtime.rs`, `oauth_state.rs`, ...). `lib.rs` keeps `SERVICE_PREFIX` and the `create_router`/`create_prefixed_router`/`create_app_routes` builders, and re-exports the feature-state entrypoints (e.g. `HttpRuntime`, `init_runtime`) so transport paths are unchanged.
- **`setup` = `app.rs` (assembly + `run`) + `config.rs` (config load + logging + run-default derivation)** behind a thin `lib.rs` (`pub mod app; pub mod config; pub use app::*; pub use config::*;`). It is the sole composition root — the only place concrete adapters are constructed.
- **`configuration` is a single `lib.rs`; `src/main.rs` stays a thin shell** that loads config, inits logging, and calls `setup::run`.
- **Dependency direction**: `domain` (no inward deps) <- `application` <- `infra`; `http` and `setup` compose them. With ports in `domain`, `infra` should not declare an `application` dependency at all.

### Preserve public APIs via re-exports

When you group flat modules into subfolders, re-export **both the moved modules and their items** at the crate root so existing flat *and* deep paths keep resolving (`domain::NormalizedIssue`, `domain::hashing::*`, `domain::summary::status`). Re-exporting the modules also keeps intra-crate `crate::<module>` references working without edits. The only import churn that should remain is the **deliberate cross-crate relocation of ports** from `application` to `domain` (callers switch `application::ports::X` -> `domain::X`).

> Knowledge sources for this section follow the project [[AGENTS]] workflow: query QMD `rustycog-wiki` first for framework behavior, treat `sonar-auto-patcher-wiki` as the functional spec, and use this guide for the layout dichotomy; IAMRusty is the worked reference.

## Common Pitfalls

- Letting `command_type()` strings drift away from registration keys.
- Mixing `AuthUser` and `OptionalAuthUser` with the wrong route mode.
- Assuming `config/default.toml` is always merged automatically.
- Defining `[command.retry]`, `logging.level`, or service timeout knobs in TOML without verifying the current runtime path actually consumes them.
- Depending on historical `rustycog-*` per-crate manifests instead of the unified `rustycog-framework` feature set.
- Calling another service's `run()` method from a modular monolith. Compose via setup/build APIs, extract routers, start only background tasks, and serve exactly one top-level router.
- Letting standalone microservice paths drift from monolith paths. The same `SERVICE_PREFIX` constants should define both modes, e.g. `/iam`, `/telegraph`, `/hive`, and `/manifesto`.
- Forgetting that the permission middleware only binds the deepest UUID-shaped path segment into `ResourceRef`. Non-UUID segments (`{component_type}`, `{resource}`) are skipped.
- Emitting a domain event without a corresponding authorization-sync path — the OpenFGA store can fall out of sync with aggregate state silently.
- Defining the trait ports in `application` instead of `domain`, which forces `infra` to depend on `application` and blurs the hexagonal boundary.
- A fat `lib.rs`, or a single-file `http`/`setup` crate, instead of thin `lib.rs` + concern subfolders (`handlers/` + `error.rs` + feature-state for `http`; `app.rs` + `config.rs` for `setup`).
- Regrouping modules into subfolders without re-exporting them at the crate root, causing needless `application::X`/`domain::Y` import churn across the whole workspace.

## Sources

- <!-- [[references/rustycog-service-construction]] --> — Combined source summary for this workflow
- <!-- [[projects/rustycog/references/index]] --> — Code-backed inventory of the feature modules this workflow wires together
- <!-- [[concepts/shared-rust-microservice-sdk]] --> — Broader platform motivation for the approach
- `AIForAll/IAMRusty` — canonical reference implementation of the crate-internal gold-standard layout (thin `lib.rs`, ports in `domain`, `handlers/`+`error.rs`+feature-state in `http`, `app.rs`+`config.rs` in `setup`)
- QMD collections per [[AGENTS]]: `rustycog-wiki` (framework behavior, queried first) and `sonar-auto-patcher-wiki` (functional spec for the DeRust port that motivated this section)