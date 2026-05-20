---
title: Hive Testing and API Fixtures
category: references
tags: [reference, testing, fixtures, visibility/internal]
sources:
  - Hive/config/test.toml
  - Hive/tests/common.rs
  - Hive/tests/organization_api_tests.rs
  - Hive/tests/members_api_tests.rs
  - Hive/tests/sqs_event_routing_tests.rs
  - Hive/tests/external_link_api_tests.rs
  - Hive/tests/fixtures/external_provider/mod.rs
  - Hive/tests/fixtures/external_provider/service.rs
  - Hive/tests/fixtures/external_provider/resources.rs
summary: Hive validates its API with real DB, JWT-backed tests, real OpenFGA, LocalStack SQS routing checks, and an ExternalProviderMockService wrapper around the shared rustycog-testing wiremock fixture.
provenance:
  extracted: 0.78
  inferred: 0.14
  ambiguous: 0.08
created: 2026-04-14T18:56:22.3888182Z
updated: 2026-04-25T11:25:00Z
---

# Hive Testing and API Fixtures

These sources show how `[[projects/hive/hive]]` validates its organization-management API: real database state, JWT-authenticated HTTP tests, shared RustyCog test-server wiring, and dedicated fixtures for the external provider dependency.

## Key Ideas

- `HiveTestDescriptor` follows the shared `rustycog_testing` pattern for service bootstrapping, migrations, DB setup, and test-server lifecycle, as documented in `[[projects/rustycog/references/rustycog-testing]]`.
- Hive's default test runtime keeps `has_db()` true and `has_sqs()` false so ordinary HTTP/API tests do not start LocalStack. `Hive/tests/sqs_event_routing_tests.rs` defines its own SQS-specific descriptor and sets `HIVE_QUEUE__ENABLED=true` before bootstrapping the fixture/server.
- `Hive/tests/sqs_event_routing_tests.rs` is one of the canonical producer-side routing checks: each `#[serial]` test drains `test-sentinel-sync-events` and `test-hive-default-events`, performs a real HTTP action, waits with `TestSqs::wait_for_messages_from_queue("test-sentinel-sync-events", ...)`, and asserts the explicitly mapped event did not fall through to the default queue.
- Organization, member, and external-link tests create real DB state, mint JWTs, call the live HTTP server, and assert on both response codes and persisted data.
- The tests are serial and mirror the same broad style now used by IAMRusty and Manifesto producer-routing tests, while Telegraph remains the queue-consumer behavior example.
- External provider behavior is isolated through an `ExternalProviderMockService` (in `Hive/tests/fixtures/external_provider/service.rs`) that wraps the shared [[projects/rustycog/references/wiremock-mock-server-fixture]] and emulates `/config/validate`, `/connection/test`, `/organization/info`, `/members`, and `/members/check` endpoints.
- The wrapper exposes one async `mock_*` method per scenario — `mock_validate_config_ok`, `mock_validate_config_fail(message_contains)`, `mock_connection_test(connected)`, `mock_organization_info(name, external_id)`, `mock_members(members)`, `mock_is_member(is_member)` — each returning `&Self` so arrangements can be chained per test.
- Each mock is mounted with `Mock::given(method("POST")).and(path("/..."))`; the failure stub also chains `body_string_contains(message_contains)` to react only to specific request bodies, and the response side uses `ResponseTemplate::new(<status>).set_body_json(...)` with typed DTOs from `Hive/tests/fixtures/external_provider/resources.rs` (`ConnectionTestResponseBody`, `OrganizationInfo`, `MembersResponse`, `Member`).
- The fixture is constructed via `ExternalProviderFixtures::service().await`, which calls `ExternalProviderMockService::new()` → `MockServerFixture::new()`; the fixture handle is held in a `_fixture` field so its `Drop` impl resets all mocks for the next test.
- Tests like `external_link_api_tests` (`create_external_link_happy_path`, `create_external_link_requires_auth`, `create_external_link_forbidden_for_read_only_member`) currently exercise the API surface end-to-end against the live HTTP server and DB but do not yet arrange `ExternalProviderMockService` stubs in the visible flows. ^[ambiguous]
- Follow [[skills/stubbing-http-with-wiremock]] when extending this fixture or adding a new external collaborator.
- IAMRusty and Manifesto now have dedicated `sqs_event_routing_tests` files with SQS-specific descriptors, so their default HTTP harnesses can stay lean while producer-routing tests opt into LocalStack explicitly. ^[inferred]

## Open Questions

- The live test suite is strong on org/member/external-link flows, but this source batch does not show a correspondingly rich invitation or sync-job API test surface. ^[ambiguous]
- The Hive routing tests validate publisher placement, not SentinelSync consumption; tuple translation coverage still belongs in `[[projects/sentinel-sync/references/event-to-tuple-mapping]]` and translator tests.

## Sources

- [[projects/hive/hive]] - Service whose API and fixtures are under test.
- [[concepts/integration-testing-with-real-infrastructure]] - Cross-service concept view of these patterns.
- [[projects/hive/references/hive-http-api-and-openapi-drift]] - Live HTTP behaviors covered by the tests.
- [[projects/hive/concepts/external-provider-sync-jobs]] - External-provider fixture behavior and sync context.
- [[projects/rustycog/references/rustycog-testing]] - Shared RustyCog testing runtime reused by Hive.
- [[projects/rustycog/references/wiremock-mock-server-fixture]] - Shared mock server the `ExternalProviderMockService` wraps.
- [[skills/stubbing-http-with-wiremock]] - Recipe behind the `mock_*` helper convention used here.