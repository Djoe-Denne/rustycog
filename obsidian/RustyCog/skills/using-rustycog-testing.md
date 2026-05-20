---
title: Using RustyCog Testing
category: skills
tags: [rustycog, testing, skills, visibility/internal]
sources:
  - rustycog/rustycog-testing/src/lib.rs
  - rustycog/rustycog-testing/src/common/test_server.rs
  - rustycog/rustycog-testing/src/common/kafka_testcontainer.rs
  - rustycog/rustycog-testing/src/common/sqs_testcontainer.rs
  - rustycog/rustycog-testing/src/wiremock/mod.rs
  - IAMRusty/tests/common.rs
  - IAMRusty/tests/sqs_event_routing_tests.rs
  - Telegraph/tests/common.rs
  - Hive/tests/common.rs
  - Hive/tests/sqs_event_routing_tests.rs
  - Manifesto/tests/common.rs
  - Manifesto/tests/sqs_event_routing_tests.rs
summary: Workflow for using rustycog-testing to bootstrap service tests, prefixed URLs, SQS fanout fixtures, real infrastructure, and wiremock fakes.
provenance:
  extracted: 0.88
  inferred: 0.08
  ambiguous: 0.04
created: 2026-04-15T17:15:56.0808743Z
updated: 2026-04-25T11:25:00Z
---

# Using RustyCog Testing

Use this guide when setting up integration tests with `<!-- [[projects/rustycog/references/rustycog-testing]] -->`.

## Workflow

- Create one service test descriptor that builds app fixtures, test DB setup, and HTTP app wiring.
- Use `setup_test_server()` to obtain reusable base URL and HTTP client for endpoint tests.
- Return a **service-prefixed** base URL from each service's local `tests/common.rs`: `/iam` for IAMRusty, `/telegraph` for Telegraph, `/hive` for Hive, and `/manifesto` for Manifesto. Test bodies should append route paths such as `/api/...` to that prefixed base URL instead of repeating the prefix at every call site.
- Add DB fixtures and migration setup in shared test initialization so each test starts from explicit state.
- Enable Kafka/SQS testcontainer helpers only for tests that need real queue behavior; keep shared `test.toml` queue settings `enabled = false` unless the whole suite genuinely needs transport.
- For SQS fanout tests, configure all destination queues in `SqsConfig`; the LocalStack fixture creates every configured physical queue and named-queue helpers let tests assert each destination independently.
- For producer-side SQS routing tests, use a distinct `default_queues` fallback plus explicit `[queue.queues]` mappings. Drain every relevant queue before the action, then assert the event appears via `wait_for_messages_from_queue(mapped_queue, ...)` and does **not** appear via `get_all_messages_from_queue(default_queue, ...)`.
- Prefer a dedicated routing-test descriptor with `has_sqs() == true` and a test-binary env override such as `HIVE_QUEUE__ENABLED=true`, `IAM_QUEUE__ENABLED=true`, or `MANIFESTO_QUEUE__ENABLED=true`. The default descriptor should keep `has_sqs() == false` so normal HTTP/API tests do not pay LocalStack startup cost.
- Keep named-queue routing tests transport-heavy and `#[serial]`. `Hive/tests/sqs_event_routing_tests.rs`, `IAMRusty/tests/sqs_event_routing_tests.rs`, and `Manifesto/tests/sqs_event_routing_tests.rs` are the reference shapes for HTTP action -> domain event -> mapped LocalStack queue assertions.
- For outbound HTTP collaborators, wrap the shared [[projects/rustycog/references/wiremock-mock-server-fixture]] in a typed `MockService` per collaborator and arrange responses with `mock_*` helpers — see [[skills/stubbing-http-with-wiremock]] for the recipe.
- For permission-gated routes (`with_permission_on`), construct [[projects/rustycog/references/openfga-mock-service]] in `setup_test_server` and return its handle alongside the test fixture so individual tests can arrange `mock_check_allow` / `mock_check_deny` per tuple. Set `openfga.cache_ttl_seconds = 0` in the test config so re-arranged decisions actually fire.
- Keep transport-heavy tests separate from fast unit tests to preserve local iteration speed.

## Common Pitfalls

- Recreating server/process setup in each test instead of reusing descriptor-based helpers.
- Using the raw origin returned by `rustycog_testing::setup_test_server()` directly in service tests. Wrap it once in the service-local helper with the same `SERVICE_PREFIX` used by runtime routing, otherwise tests will pass against paths that do not match microservice or monolith mode.
- Hard-coding `/api/...` against a bare origin in new test helpers. Keep the prefix centralized in `tests/common.rs` so moving between standalone and monolith runtime modes does not change individual tests.
- Leaving queue tests enabled by default when suites do not need transport behavior.
- Checking only the fallback queue in fanout or routing tests. Use named-queue reads when one event should land in multiple destination queues, and assert the fallback queue is empty when a per-event mapping should bypass it.
- Forgetting to reset state between tests when reusing shared server instances.
- Skipping `#[serial]` on tests that touch the wiremock fixture — the singleton listens on a fixed port and mocks are process-wide, so parallel tests will clobber each other.
- Asserting on a `403` from a permission-gated route without resetting the wiremock fake first when `setup_test_server` mounted a permissive `mock_check_any(true)` default. wiremock matches in registration order, so the catch-all wins. Call `openfga.reset().await` then mount your `mock_check_deny(...)` for the exact tuple under test.
- Asserting on a state change after a revoke/grant API call without setting `openfga.cache_ttl_seconds = 0`. The production `CachedPermissionChecker` (15s TTL) will serve the pre-revoke decision and the second request never reaches the wiremock fake.

## Sources

- [[projects/rustycog/references/rustycog-testing]]
- [[projects/aiforall/skills/running-aiforall-runtime-modes]]
- [[projects/aiforall/references/modular-monolith-runtime]]
- [[projects/rustycog/references/wiremock-mock-server-fixture]]
- [[projects/rustycog/references/openfga-mock-service]]
- [[skills/stubbing-http-with-wiremock]]
- [[skills/using-rustycog-permission]]
- [[concepts/integration-testing-with-real-infrastructure]]
- [[projects/rustycog/rustycog]]
- [[projects/manifesto/references/manifesto-testing-and-fixtures]]
