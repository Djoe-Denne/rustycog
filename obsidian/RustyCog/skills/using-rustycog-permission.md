---
title: Using RustyCog Permission
category: skills
tags: [rustycog, permissions, openfga, skills, visibility/internal]
sources:
  - rustycog/rustycog-permission/src/lib.rs
  - rustycog/rustycog-permission/src/checker.rs
  - rustycog/rustycog-http/src/middleware_permission.rs
  - openfga/model.fga
summary: Procedure for wiring an OpenFGA-backed PermissionChecker into a service and adding centralized authorization checks to routes, including the cache TTL knob and the wiremock-backed test fixture.
updated: 2026-04-22T17:30:00Z
---

# Using RustyCog Permission

Use this guide when integrating [[projects/rustycog/references/rustycog-permission]] into a service.

## Workflow

- Build an `OpenFgaPermissionChecker` from `OpenFgaClientConfig` in your composition root.
- Read `config.openfga.cache_ttl_seconds` and skip the `CachedPermissionChecker` decoration entirely when it is `Some(0)`; otherwise wrap with the configured TTL (default 15s when `None`). Then wrap with `MetricsPermissionChecker` before storing the result in `AppState`.
- Pass that single `Arc<dyn PermissionChecker>` into `AppState::new(command_service, user_id_extractor, checker)`.
- On every guarded route call `.with_permission_on(Permission::X, "<openfga_type>")` — the only authz knob.
- Make sure each guarded route uses a UUID path parameter; middleware only binds the **deepest** UUID into `ResourceRef`. For routes like `/api/projects/{project_id}/components/{component_id}`, the resource is the component id, not the project id — important when arranging stubs in tests.
- For unit tests, use `InMemoryPermissionChecker` and explicit `allow(...)` calls. For integration tests that boot the real service, use [[projects/rustycog/references/openfga-mock-service]] (`OpenFgaFixtures::service().await`) and arrange per-tuple decisions via `mock_check_allow` / `mock_check_deny`.

## Test config

When wiring [[projects/rustycog/references/openfga-mock-service]] into a service's integration test config:

- Point `openfga.api_url` at `http://127.0.0.1:3000` (the singleton wiremock listener).
- Pin `openfga.store_id` to the value the fixture defaults to (`01h0test0store0fixture000openfga`) or call `OpenFgaFixtures::service_with_store_id(...)` and align the config.
- Set `openfga.cache_ttl_seconds = 0` so `Check` is re-issued on every middleware invocation. This is what makes grant ➜ revoke ➜ deny tests observe the second decision; without it, the cached allow from the first request masks the revoke.

## Common pitfalls

- Naming `object_type` for something that does not exist in [openfga/model.fga](../../../openfga/model.fga). The check fails closed with a logged 4xx from OpenFGA.
- Building a fresh checker per request. The composition root must build it once.
- Assuming an empty `InMemoryPermissionChecker` allows by default — it denies everything until you call `allow`.
- Forgetting to publish the matching domain event so [[projects/sentinel-sync/sentinel-sync]] can write the corresponding tuple. Routes will silently 403 until the tuple arrives.
- Leaving `cache_ttl_seconds` at its `None` default in test configs and then wondering why a `mock_check_deny` arranged after a successful `mock_check_allow` for the same tuple never fires — the cache served the stale allow. Set it to `Some(0)` for tests.
- Mounting per-tuple deny stubs on top of a permissive `mock_check_any(true)` default without resetting first. wiremock matches in registration order; the catch-all wins. Call `openfga.reset().await` in the test before mounting the deny.

## Source files

- `rustycog/rustycog-permission/src/lib.rs`
- `rustycog/rustycog-permission/src/checker.rs`
- `rustycog/rustycog-http/src/builder.rs`
- `rustycog/rustycog-http/src/middleware_permission.rs`
- `openfga/model.fga`

## Key types

- `PermissionChecker` — async trait `check(subject, action, resource) -> Result<bool, DomainError>`.
- `OpenFgaPermissionChecker` — production implementation.
- `CachedPermissionChecker` — short-TTL LRU decorator (`moka`).
- `MetricsPermissionChecker` — `tracing`-instrumented decorator emitting per-decision events.
- `InMemoryPermissionChecker` — test-only checker.
- `Subject`, `ResourceRef`, `ResourceId` — authorization primitives.

## Sources

- [[projects/rustycog/references/rustycog-permission]]
- [[projects/rustycog/references/rustycog-http]]
- [[projects/rustycog/references/openfga-mock-service]]
- [[skills/stubbing-http-with-wiremock]]
- [[entities/permission-checker]]
- [[concepts/openfga-as-authorization-engine]]
- [[concepts/centralized-authorization-service]]
- [[projects/sentinel-sync/sentinel-sync]]
- [[projects/manifesto/references/manifesto-testing-and-fixtures]]
