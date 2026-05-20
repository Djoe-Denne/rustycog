---
title: OpenFGA Mock Service
category: references
tags: [reference, rustycog, testing, fixtures, wiremock, openfga, permissions, visibility/internal]
sources:
  - rustycog/rustycog-testing/src/permission/mod.rs
  - rustycog/rustycog-testing/src/permission/service.rs
  - rustycog/rustycog-testing/src/permission/resources.rs
  - rustycog/rustycog-permission/src/checker.rs
  - Manifesto/setup/src/app.rs
  - Manifesto/config/test.toml
  - Manifesto/tests/common.rs
  - Manifesto/tests/component_api_tests.rs
summary: >-
  Historical notes for the old wiremock-backed OpenFGA Check fake; Hive, Manifesto, and Telegraph integration tests now use a real OpenFGA testcontainer.
provenance:
  extracted: 0.8
  inferred: 0.08
  ambiguous: 0.12
created: 2026-04-22T17:30:00Z
updated: 2026-05-20T14:06:00Z
---

# OpenFGA Mock Service

> [!warning] Historical
> This page documents the removed wiremock-backed `OpenFgaMockService`. Hive, Manifesto, and Telegraph integration tests now use [[projects/rustycog/references/openfga-real-testcontainer-fixture]], which exercises real OpenFGA store/model/tuple semantics and denies by default.

`rustycog_testing::permission::OpenFgaMockService` is the wiremock-backed fake of OpenFGA's `POST /stores/{store_id}/check` endpoint. It wraps the shared `[[projects/rustycog/references/wiremock-mock-server-fixture]]` and lets integration tests answer permission-checker `Check` calls per-tuple instead of standing up a real OpenFGA store and a `sentinel-sync` worker.

It is the third concrete consumer of the shared singleton wiremock server (alongside Hive's `ExternalProviderMockService` and Telegraph's `SmtpService`) but it is special because it lives **inside** `rustycog-testing` itself, not in a service's `tests/fixtures/` tree — every service that wires `[[projects/rustycog/references/rustycog-permission]]` can reuse it directly.

## Module Anatomy

- `OpenFgaFixtures::service().await` — preferred factory; bound to a deterministic store id `01h0test0store0fixture000openfga` so paths stay predictable.
- `OpenFgaFixtures::service_with_store_id(...)` — pin a specific store id when the test needs to exercise the store-id-from-config code path.
- `OpenFgaMockService` holds both the `Arc<MockServer>` (for mounting) and the `MockServerFixture` (in a `_fixture` field, so its `Drop` runs the post-test reset).
- `client_config()` returns a ready-made `OpenFgaClientConfig` pointing at the fake with `cache_ttl_seconds = Some(0)` baked in — drop straight into `OpenFgaPermissionChecker::new(...)` for ad-hoc test wiring.

## API Surface

- `mock_check_allow(subject, action, resource) -> &Self` — answer `{"allowed": true}` for that exact tuple.
- `mock_check_deny(subject, action, resource) -> &Self` — answer `{"allowed": false}` for that exact tuple.
- `mock_check_allow_wildcard(action, resource) -> &Self` — model a public-read tuple (`...#viewer@user:*`) by allowing the wildcard subject for the given action+resource. Pairs with `optional_permission_middleware`'s wildcard fallback for anonymous callers — see [[concepts/anonymous-public-read-via-wildcard-subject]].
- `mock_check_deny_wildcard(action, resource) -> &Self` — explicit anonymous-deny stub. Useful for tests that simulate `sentinel-sync` removing the wildcard tuple after a project flips public → private.
- `mock_check_any(allow: bool) -> &Self` — catch-all answering every `Check` against the configured store with the same decision. Useful as a permissive default in `setup_test_server`.
- `mock_check_error(status, body) -> &Self` — return a non-success status to drive the `OpenFGA Check returned <status>` error path through the production checker.
- `mock_check_requires_bearer(token, allow) -> &Self` — only answer when the request carries the matching `Authorization: Bearer <token>` header. Used to verify that the production checker forwards `OpenFgaClientConfig::api_token`.
- `reset()` — wipe every previously mounted `Check` stub. Required when a test needs to **override** a stub mounted earlier in the same test or by `setup_test_server` — wiremock matches in registration order so a later `mock_check_deny` would never fire ahead of an existing `mock_check_any(true)`.
- `received_requests()` / `received_check_requests()` / `check_count()` / `verify_check_called(subject, action, resource)` — inspection helpers built on `MockServer::received_requests()`. The `verify_check_called` helper decodes the request body and compares the OpenFGA tuple, not the raw bytes — pass `Subject::wildcard()` to assert that anonymous callers actually reached the checker.

## Stub Patterns

### Permissive default (happy-path tests)

Mount once in the harness, never reset:

```rust
let openfga = OpenFgaFixtures::service().await;
openfga.mock_check_any(true).await;
```

This is what `Manifesto/tests/common.rs::setup_test_server` does. Tests that hit permission-gated routes pass through the route guard with no per-test arrangement.

### Denial test

Reset the catch-all first, then mount the per-tuple deny:

```rust
openfga.reset().await;
openfga
    .mock_check_deny(
        Subject::new(member_id),
        Permission::Admin,
        ResourceRef::new("project", component.id()),
    )
    .await;
```

The reset is mandatory — without it, the catch-all permissive default mounted by `setup_test_server` matches first and the deny is dead code.

### Multi-tuple test (e.g. component-specific permission)

Reset, then mount each (subject, action, resource) tuple the test asserts on:

```rust
openfga.reset().await;
openfga
    .mock_check_allow(Subject::new(owner_id), Permission::Admin, ResourceRef::new("project", c1.id())).await
    .mock_check_allow(Subject::new(member_id), Permission::Admin, ResourceRef::new("project", c1.id())).await
    .mock_check_deny(Subject::new(member_id), Permission::Admin, ResourceRef::new("project", c2.id())).await;
```

Different resource UUIDs become different cache keys in `CachedPermissionChecker`, so distinct decisions don't collide even with caching on.

### Grant ➜ revoke ➜ deny test

This shape requires **two** mock arrangements separated by the API call that performs the revoke, **plus** `cache_ttl_seconds = 0` in the test config so the second `Check` actually re-issues:

```rust
// Phase 1 — grant active, member can administer
openfga.reset().await;
openfga
    .mock_check_allow(owner_subject, Admin, project_component_resource).await
    .mock_check_allow(member_subject, Admin, project_component_resource).await;
// ... POST grant, PATCH (200), DELETE revoke ...

// Phase 2 — simulate sentinel-sync propagating the revoke
openfga.reset().await;
openfga
    .mock_check_allow(owner_subject, Admin, project_component_resource).await
    .mock_check_deny(member_subject, Admin, project_component_resource).await;
// ... PATCH (403) ...
```

Without `cache_ttl_seconds = 0`, the second PATCH would hit the `CachedPermissionChecker` entry from Phase 1 and never reach the re-arranged mock. See [[projects/rustycog/references/rustycog-permission]] for the new field's semantics.

## Resource ID Extraction Quirk

The `with_permission_on(Permission::X, "<type>")` middleware uses the **trailing UUID** of the request path as the `ResourceRef::object_id`, regardless of how many UUIDs precede it. In Manifesto, that means:

- `/api/projects/{project_id}` → resource `<type>:project_id`
- `/api/projects/{project_id}/components/{component_id}` → resource `<type>:component_id`
- `/api/projects/{project_id}/members/{user_id}/permissions/component/{component_id}` → resource `<type>:component_id`

When mounting `mock_check_*` stubs, use the trailing UUID from the URL the test will issue, **not** the project id. ^[inferred from Manifesto trace logs at line 6 of the failing-test capture]

## Cache TTL Companion Setting

`OpenFgaClientConfig.cache_ttl_seconds` (added to `[[projects/rustycog/references/rustycog-permission]]` for this fixture's benefit) controls whether `CachedPermissionChecker` is wired in front of the production `OpenFgaPermissionChecker`:

- `None` (default) — production behavior, 15s TTL.
- `Some(15)` — explicit production TTL override.
- `Some(0)` — skip the cache decoration entirely. **Required** in any test config that relies on the wiremock fake to flip a decision mid-test.

`Manifesto/setup/src/app.rs` reads it; other services adopting the fixture must do the same (or accept that grant-revoke-deny shapes won't work in their tests).

## Isolation Semantics

- Construction goes through `MockServerFixture::new()` which eagerly resets the shared singleton. Each test starts with a clean board even if a prior test forgot to reset.
- `Drop for MockServerFixture` schedules another async reset on the current Tokio runtime as a belt-and-suspenders pass.
- Tests must be marked `#[serial]`. The wiremock server is bound to `127.0.0.1:3000` and shared across the test process — parallel runs would clobber each other's stubs and fight for the port.

## Open Questions

- The fixture currently models only `Check`. OpenFGA's `Write`, `Read`, `ListObjects`, and `Expand` endpoints are not stubbed; tests that exercise those code paths through `OpenFgaPermissionChecker` would still hit a 404 from the bare wiremock server. ^[ambiguous]
- The deterministic default store id is convenient but couples the fixture to whatever value `Manifesto/config/test.toml` sets for `openfga.store_id`. Services adopting the fixture must either reuse the same constant or call `service_with_store_id(...)` and align their config. ^[inferred]
- `client_config()` always sets `cache_ttl_seconds = Some(0)`, which is the right default for ad-hoc test wiring but may surprise a caller who reads the field expecting the production 15s. ^[inferred]

## See Also

- [[projects/rustycog/references/wiremock-mock-server-fixture]] — underlying singleton.
- [[projects/rustycog/references/rustycog-permission]] — the production checker stack and the new `cache_ttl_seconds` field.
- [[concepts/anonymous-public-read-via-wildcard-subject]] — how `mock_check_allow_wildcard` ties into the production wildcard-subject flow.
- [[skills/stubbing-http-with-wiremock]] — recipe for authoring sibling fixtures (Hive/Telegraph style).
- [[skills/using-rustycog-permission]] — wiring the production checker into a service.
- [[projects/manifesto/references/manifesto-testing-and-fixtures]] — canonical consumer of this fixture today.
- [[concepts/integration-testing-with-real-infrastructure]] — when to mock vs. when to run a real backing service.
