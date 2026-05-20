---
title: Stubbing HTTP with Wiremock
category: skills
tags: [skills, testing, wiremock, fixtures, visibility/internal]
sources:
  - rustycog/rustycog-testing/src/wiremock/mod.rs
  - Hive/tests/fixtures/external_provider/service.rs
  - Hive/tests/fixtures/external_provider/mod.rs
  - Telegraph/tests/fixtures/smtp/service.rs
  - Telegraph/tests/fixtures/smtp/mod.rs
summary: Recipe for stubbing outbound HTTP in service integration tests by wrapping rustycog-testing's shared MockServerFixture with a per-collaborator helper struct, including the per-test reset pattern needed when overriding stubs mounted by setup_test_server.
provenance:
  extracted: 0.7
  inferred: 0.24
  ambiguous: 0.06
created: 2026-04-22T16:20:59Z
updated: 2026-04-22T17:30:00Z
---

# Stubbing HTTP with Wiremock

Use this recipe when a service test needs to fake an external HTTP collaborator. The shared `MockServerFixture` from `[[projects/rustycog/references/wiremock-mock-server-fixture]]` already handles the listener, lifecycle, and reset semantics — your job is to wrap it in a typed helper that exposes one method per scenario you care about.

## Workflow

1. **Add a fixture module** under `<service>/tests/fixtures/<collaborator>/` with `mod.rs`, `service.rs`, and (optionally) `resources.rs` for the request/response DTOs. Hive's `external_provider` and Telegraph's `smtp` are the canonical examples.
2. **Hold both the `Arc<MockServer>` and the `MockServerFixture`** in your service struct. The fixture is kept in a `_fixture` field purely so its `Drop` impl runs at the right moment:
   ```rust
   pub struct MyCollaboratorMockService {
       server: Arc<MockServer>,
       _fixture: MockServerFixture,
   }
   ```
3. **Construct via `MockServerFixture::new().await`** in an async `new()` constructor. The fixture's constructor already calls `reset_all_mocks()`, so each test starts with a clean board.
4. **Expose one async `mock_*` method per scenario.** Return `&Self` so callers can chain arrangements:
   ```rust
   pub async fn mock_validate_config_ok(&self) -> &Self {
       Mock::given(method("POST"))
           .and(path("/config/validate"))
           .respond_with(ResponseTemplate::new(200).set_body_json(json!({"ok": true})))
           .mount(&*self.server)
           .await;
       self
   }
   ```
5. **Expose `base_url()`** (or `host()` / `port()` / `uri()` for protocol-shaped fakes) so the service-under-test can be configured to point at the fake.
6. **Wrap construction in a namespace struct** like `ExternalProviderFixtures` or `SmtpFixtures` with an `async fn service()` factory. The test code then reads as `let provider = ExternalProviderFixtures::service().await;`.
7. **Compose multi-step scenarios** as additional async methods (`mock_successful_email_send`) that call the lower-level `mock_*` helpers in sequence. For ad-hoc flows, expose a builder (`SmtpScenarioBuilder`) instead.

## Matcher Conventions

- Always pin both `method(...)` and `path(...)`. The shared server is process-wide, so loose matches will catch unrelated requests from other fixtures arranged in the same test.
- Use `body_string_contains(...)` for body-aware stubs. For text bodies that may include template variation, match on a small set of significant words (Telegraph's `mock_data` takes the first three words longer than two characters from the expected text body).
- Use `header(...)` and `query_param(...)` from `wiremock::matchers` when the collaborator distinguishes responses on those axes.
- Map error scenarios by setting the response status explicitly: `ResponseTemplate::new(if response.code >= 400 { 400 } else { 200 })`. `set_body_json` then carries the structured error payload.

## Overriding Stubs Mid-Test (Reset Pattern)

wiremock matches stubs in **registration order, first-match wins**. That has two consequences for harnesses that mount a permissive default in `setup_test_server`:

1. A per-tuple deny mounted *after* the catch-all will never fire — the catch-all matches first.
2. A test that needs to flip behavior partway (e.g. grant ➜ revoke ➜ deny) cannot do so by mounting another stub on top.

The fix is to expose a `reset()` method on the fixture wrapper that delegates to `MockServerFixture::reset()`, then have the test wipe the catch-all and mount only the stubs it cares about:

```rust
openfga.reset().await;                              // wipe the permissive default
openfga
    .mock_check_deny(member_subject, Permission::Admin, project_resource).await;
```

For grant ➜ revoke ➜ deny shapes, repeat the reset between phases:

```rust
// Phase 1 — grant active
openfga.reset().await;
openfga.mock_check_allow(member_subject, Admin, resource).await;
// ... PATCH (200), DELETE revoke ...

// Phase 2 — simulate the revoke propagating
openfga.reset().await;
openfga.mock_check_deny(member_subject, Admin, resource).await;
// ... PATCH (403) ...
```

If your fixture wrapper holds the `MockServerFixture` in a private `_fixture` field, expose `reset()` from the impl: `pub async fn reset(&self) { self._fixture.reset().await; }`. `[[projects/rustycog/references/openfga-mock-service]]` is the canonical example.

## Inspection

When a test needs to assert on what the service actually sent, use the upstream `MockServer::received_requests().await`. Telegraph wraps this in `received_requests`, `verify_email_sent(subject, recipient)`, and `email_count()` so test bodies stay declarative.

## Common Pitfalls

- **Forgetting `#[serial]`.** The wiremock server is shared across the whole test process and bound to a fixed port. Tests that run in parallel will see each other's mocks and clobber each other's port reservation. Use `serial_test::serial` like Hive's `external_link_api_tests` and Telegraph's notification tests do.
- **Dropping the fixture outside a Tokio runtime.** The auto-reset on `Drop` only fires if `tokio::runtime::Handle::try_current()` succeeds. Synchronous teardown paths skip cleanup silently.
- **Holding only the `Arc<MockServer>`.** If you discard the `MockServerFixture`, you lose the drop-time reset. Always keep it in a `_fixture` field even if nothing reads it.
- **Mounting the same `Mock` twice across tests.** Without a `MockServerFixture::new()` per test, prior arrangements stack up because wiremock matches the first registered mock that fits a request. The constructor's eager reset is what prevents this — don't bypass it by constructing helpers from `get_mock_server()` directly.
- **Assuming the listener can move.** Port `127.0.0.1:3000` is hard-coded in `wiremock/mod.rs`; if a service-under-test needs a different host or port, point its config at the fake's `base_url()`/`uri()` rather than trying to relocate the server. ^[inferred]
- **Mounting a per-tuple deny on top of a catch-all allow.** First-match-wins means the catch-all swallows the request before the deny is considered. Always `reset()` first when overriding a default.
- **Forgetting downstream caching.** If the production code wraps the wiremock-faked HTTP call in a cache (`CachedPermissionChecker` is the canonical example), a second request for the same key never reaches the fake — the test sees the stale answer regardless of how the mocks are arranged. Make the cache TTL configurable and set it to 0 in test configs (see [[projects/rustycog/references/rustycog-permission]]'s `cache_ttl_seconds` for the pattern).

## When Not To Use This

If the test is verifying protocol-level behavior of the real collaborator (SMTP framing, Kafka ack semantics, Postgres SQL behavior), reach for a real testcontainer instead — the recipe is in [[skills/creating-testcontainer-fixtures]]. Telegraph keeps both `SmtpService` (wiremock) and `TestSmtp` (MailHog testcontainer) in the same fixture tree precisely because they answer different questions.

## Sources

- [[projects/rustycog/references/wiremock-mock-server-fixture]] — fixture API surface and isolation semantics.
- [[projects/rustycog/references/openfga-mock-service]] — in-crate consumer with the canonical reset-and-deny pattern, plus the `cache_ttl_seconds = 0` companion setting.
- [[skills/creating-testcontainer-fixtures]] — sister skill for the real-protocol testcontainer alternative.
- [[projects/rustycog/references/rustycog-testing]] — parent crate where the fixture lives.
- [[projects/hive/references/hive-testing-and-api-fixtures]] — Hive's `ExternalProviderMockService` recipe.
- [[projects/telegraph/references/telegraph-testing-and-smtp-fixtures]] — Telegraph's `SmtpService` recipe and scenario builder.
- [[concepts/integration-testing-with-real-infrastructure]] — when to mock vs. when to run a real container.
