---
title: >-
  Manifesto Testing and Fixtures
category: references
tags: [reference, testing, fixtures, visibility/internal]
sources:
  - Manifesto/tests/common.rs
  - Manifesto/tests/public_acl_api_tests.rs
  - Manifesto/tests/component_acl_consistency_tests.rs
  - Manifesto/tests/component_service_client_tests.rs
  - Manifesto/tests/event_runtime_tests.rs
  - Manifesto/tests/project_api_tests.rs
  - Manifesto/tests/component_api_tests.rs
  - Manifesto/tests/member_api_tests.rs
  - Manifesto/tests/fixtures/db/mod.rs
  - Manifesto/setup/src/app.rs
  - rustycog/rustycog-http/tests/permission_middleware_tests.rs
  - Manifesto/config/test.toml
  - rustycog/rustycog-testing/src/permission/service.rs
summary: >-
  Manifesto-specific testing notes on top of RustyCog's shared harness, covering DB-backed API suites,
  focused tests for auth/ACL/fail-closed integrations, and the wiremock-backed OpenFgaMockService
  wired through setup_test_server with cache_ttl_seconds = 0 for deterministic permission decisions.
provenance:
  extracted: 0.85
  inferred: 0.10
  ambiguous: 0.05
created: 2026-04-19T11:49:06.1450368Z
updated: 2026-04-22T17:30:00Z
---

# Manifesto Testing and Fixtures

This page narrows `[[projects/rustycog/references/rustycog-testing]]` to the way `[[projects/manifesto/manifesto]]` actually uses the shared harness, fixtures, and focused remediation-era tests.

## RustyCog Baseline

- `[[projects/rustycog/references/rustycog-testing]]` explains the shared test server, migration hooks, JWT helpers, and fixture model that Manifesto builds on.
- `[[concepts/integration-testing-with-real-infrastructure]]` captures the broader pattern of using a real server plus real backing infrastructure instead of a mocked HTTP shell.

## Service-Specific Differences

- `ManifestoTestDescriptor` plugs into `rustycog-testing`, runs migrations up and down, reports `has_db() == true`, and keeps `has_sqs() == false` in the default harness.
- `setup_test_server()` boots the service through `build_and_run()`, then returns a 4-tuple: `TestFixture`, base URL, `reqwest` client, and an [[projects/rustycog/references/openfga-mock-service]] handle. The harness pre-mounts a permissive `mock_check_any(true)` catch-all so happy-path tests need no per-tuple arrangement.
- `project_api_tests.rs`, `component_api_tests.rs`, and `member_api_tests.rs` still cover the main HTTP CRUD and permission surfaces.
- `rustycog-http/tests/permission_middleware_tests.rs` now includes signed-token rejection coverage, so the shared auth middleware is tested against tampered bearer tokens instead of only happy paths.
- `tests/public_acl_api_tests.rs` covers anonymous public-read permission behavior plus project-list filter forwarding at the service boundary.
- `tests/component_acl_consistency_tests.rs` covers fail-hard component-instance ACL synchronization on add/remove flows.
- `tests/component_service_client_tests.rs` covers fail-closed component-service behavior and bearer API-key usage.
- `tests/event_runtime_tests.rs` covers disabled queue bootstrap, enabled-config no-op fallback when no broker fixture exists, and `ComponentStatusProcessor` duplicate-delivery/stale-event idempotency plus state updates.
- Tests use real signed JWTs from `rustycog_testing::http::jwt::create_jwt_token()`.
- `DbFixtures` still provides reusable builders for projects, components, and members when DB-backed scenarios are useful.

## OpenFGA Wiremock Wiring

Manifesto is the canonical consumer of [[projects/rustycog/references/openfga-mock-service]]. The relevant wiring lives in three files and is worth reading as a unit:

- `Manifesto/config/test.toml` — points `openfga.api_url = "http://127.0.0.1:3000"`, pins `openfga.store_id = "01h0test0store0fixture000openfga"` (the fixture default), and sets `openfga.cache_ttl_seconds = 0`. The last line is what makes flows that revoke a permission mid-test actually observe the new decision.
- `Manifesto/setup/src/app.rs` — reads `config.openfga.cache_ttl_seconds`, defaults to 15s when `None`, and **skips the `CachedPermissionChecker` decoration entirely when 0**. Production behavior is unchanged.
- `Manifesto/tests/common.rs` — `setup_test_server()` constructs `OpenFgaFixtures::service().await` *before* booting the app, mounts `mock_check_any(true)` as the permissive default, and returns the handle so individual tests can `reset()` and arrange specific decisions.

### Test arrangement patterns

| Test shape | Arrangement |
|---|---|
| Happy path (member is owner / admin) | None — the harness default suffices. |
| Denial test (member should be 403) | `openfga.reset().await; openfga.mock_check_deny(member_subject, Permission::Admin, ResourceRef::new("project", component.id())).await;` |
| Multi-tuple (allow on c1, deny on c2) | `reset()` then chain `mock_check_allow` for the allowed tuples and `mock_check_deny` for the denied ones — different resource UUIDs ⇒ different cache keys, so distinct decisions don't collide. |
| Grant ➜ revoke ➜ deny | Two arrangements separated by the revoke API call: Phase 1 mounts allow for owner + member; mid-flow `reset()`; Phase 2 mounts allow for owner + **deny** for member. Requires `cache_ttl_seconds = 0`. |
| Anonymous public-read (Phase 2 work — not yet wired) | `openfga.mock_check_allow_wildcard(Permission::Read, project_resource).await;` — models a `viewer@user:*` tuple. Today the 3 anonymous GET tests (`test_get_project_returns_200_for_existing_project`, `test_get_project_returns_404_for_nonexistent_project`, `test_get_project_detail_returns_200_with_components`) are temporarily authenticated instead, pending [[concepts/anonymous-public-read-via-wildcard-subject]] Phase 2. |

### Resource ID extraction quirk

`with_permission_on(Permission::X, "project")` uses the **trailing UUID** of the request path as the `ResourceRef::object_id`. For Manifesto:

- `/api/projects/{project_id}/components/{component_id}` → resource is `project:component_id` (not `project:project_id`).
- `/api/projects/{project_id}/members/{user_id}/permissions/component/{component_id}` → resource is `project:component_id`.

Tests must arrange stubs against the **trailing UUID**, not the project id. ^[inferred from middleware trace logs]

## Notes

- Checked-in configs keep queues disabled by default, so event-path confidence currently comes from focused runtime tests rather than queue-backed end-to-end API suites.
- `ComponentResponse.endpoint` and `access_token` remain unset in current API behavior, and tests treat that as the present product boundary rather than a missing fixture detail.

## Open Questions

- If queue-backed CI becomes standard later, which Manifesto event paths deserve full broker-backed integration coverage instead of today's unit-level runtime checks?
- The 3 GET tests (`test_get_project_returns_200_for_existing_project`, `test_get_project_returns_404_for_nonexistent_project`, `test_get_project_detail_returns_200_with_components`) authenticate today even though their assertion intent is endpoint-behavior, not authentication. Phase 2 of [[concepts/anonymous-public-read-via-wildcard-subject]] will revert them to anonymous and arrange `openfga.mock_check_allow_wildcard(Permission::Read, project_resource)` instead.

## Sources

- [[projects/manifesto/manifesto]] - Service hub and current MVP framing.
- [[projects/manifesto/references/manifesto-api-and-permission-flows]] - HTTP entrypoints exercised by these tests.
- [[projects/manifesto/concepts/project-ownership-and-publication-lifecycle]] - Lifecycle behavior validated by the project suite.
- [[projects/manifesto/concepts/component-instance-permissions]] - Permission model exercised by the component and member suites.
- [[concepts/integration-testing-with-real-infrastructure]] - Shared real-server testing pattern that Manifesto follows.
- [[projects/rustycog/references/openfga-mock-service]] - Wiremock-backed OpenFGA fake wired into Manifesto's test harness.
- [[projects/rustycog/references/rustycog-permission]] - Production checker stack and the `cache_ttl_seconds` field Manifesto's test config sets to 0.
- [[skills/stubbing-http-with-wiremock]] - Recipe behind the reset-and-arrange test pattern documented above.
