---
title: Wiremock MockServerFixture
category: references
tags: [reference, rustycog, testing, fixtures, wiremock, visibility/internal]
sources:
  - rustycog/rustycog-testing/src/wiremock/mod.rs
  - rustycog/rustycog-testing/src/permission/service.rs
  - Hive/tests/fixtures/external_provider/service.rs
  - Hive/tests/fixtures/external_provider/mod.rs
  - Hive/tests/fixtures/external_provider/resources.rs
  - Telegraph/tests/fixtures/smtp/service.rs
  - Telegraph/tests/fixtures/smtp/mod.rs
summary: How rustycog::testing's wiremock module exposes a single shared MockServer on port 3000 with auto-reset isolation, and helper fixtures that wrap it.
provenance:
  extracted: 0.78
  inferred: 0.16
  ambiguous: 0.06
created: 2026-04-22T16:20:59Z
updated: 2026-05-20T14:05:00Z
---

# Wiremock MockServerFixture

`rustycog::testing` exposes a single shared `wiremock::MockServer` through the `MockServerFixture` type so that any service test can stub outbound HTTP without each suite spinning up its own listener. The module lives at `rustycog/rustycog-testing/src/wiremock/mod.rs` and is re-exported from the framework as `rustycog::testing::wiremock`.

## Module Anatomy

- A `tokio::sync::OnceCell<Arc<MockServer>>` holds the server for the lifetime of the test process.
- The first caller of `get_mock_server()` binds a `std::net::TcpListener` to `127.0.0.1:3000` and starts the wiremock server on it; later callers receive a clone of the same `Arc<MockServer>`.
- An `AtomicBool` (`CLEANUP_REGISTERED`) gates a one-time cleanup wiring step that registers a `ctrlc` handler and a libc `atexit` callback. Both currently only log on shutdown — they do not run async cleanup.
- `reset_all_mocks()` calls `MockServer::reset()` if the singleton has been initialized; this is the primary isolation knob.
- `MockServerFixture::new()` fetches the singleton, immediately calls `reset_all_mocks()`, then stores the `Arc<MockServer>` so the test owns a fixture handle.
- `Drop for MockServerFixture` schedules an async `server.reset().await` on the current Tokio runtime so the next fixture starts clean even if the test forgot to reset manually.

## Public API Surface

- `get_mock_server() -> Arc<MockServer>` — singleton accessor, async.
- `get_mock_base_url() -> String` — convenience wrapper returning `server.uri()`.
- `reset_all_mocks()` — clears every mounted mock on the shared server.
- `MockServerFixture::new() -> Self` — fetch + reset; intended construction path.
- `MockServerFixture::server() -> Arc<MockServer>` — clone the inner `Arc` for mounting mocks against.
- `MockServerFixture::base_url() -> String` — same as `server.uri()`, returned by value.
- `MockServerFixture::reset()` — manual reset (in addition to the drop-time reset).

## Stub Patterns Used in This Repo

Mocks are mounted with the upstream `wiremock` crate's builder API against `&*fixture.server()`:

```rust
Mock::given(method("POST"))
    .and(path("/config/validate"))
    .respond_with(ResponseTemplate::new(200).set_body_json(json!({"ok": true})))
    .mount(&*self.server)
    .await;
```

Three service-specific wrappers in this repo show the conventional shape:

- **Hive** — `ExternalProviderMockService` (`Hive/tests/fixtures/external_provider/service.rs`) holds an `Arc<MockServer>` plus the `MockServerFixture` (kept in a `_fixture` field purely for drop-time cleanup) and exposes one async `mock_*` method per emulated provider endpoint: `mock_validate_config_ok`, `mock_validate_config_fail(message_contains)`, `mock_connection_test(connected)`, `mock_organization_info(name, external_id)`, `mock_members(members)`, `mock_is_member(is_member)`. Each method returns `&Self` so calls can be chained when arranging a scenario.
- **Telegraph** — `SmtpService` (`Telegraph/tests/fixtures/smtp/service.rs`) follows the same `_fixture` ownership pattern but goes further: in addition to one `mock_*` method per SMTP verb (`mock_greeting`, `mock_ehlo`, `mock_auth`, `mock_mail_from`, `mock_rcpt_to`, `mock_data`, `mock_quit`), it composes high-level scenarios (`mock_successful_email_send`, `mock_authenticated_email_send`, `mock_auth_failure`, `mock_recipient_rejection`) and exposes a `SmtpScenarioBuilder` for ad-hoc multi-step flows. It also adds inspection helpers (`received_requests`, `verify_email_sent`, `email_count`) on top of `MockServer::received_requests`.
- **OpenFGA (in-framework)** — the OpenFGA fixtures live inside `rustycog::testing` rather than a service's `tests/fixtures/`, so every consumer of `[[projects/rustycog/references/rustycog-permission]]` can reuse them. See [[projects/rustycog/references/openfga-real-testcontainer-fixture]] for the real-container surface.

## Request Matching Conventions

- `method("POST")` + `path("/...")` is the baseline match — every observed wrapper in the repo posts to a fake REST-shaped path even when the underlying protocol is not HTTP (Telegraph models SMTP verbs as `POST /smtp/<verb>` requests). ^[inferred]
- `body_string_contains(...)` is the dominant body matcher for both negative scenarios (Hive's `mock_validate_config_fail`) and content-aware stubs (Telegraph's `mock_data` walks the first three words >2 chars from the expected text body and adds one matcher per word).
- Header matchers (`header(...)`) and `query_param(...)` are imported in Telegraph's `service.rs` but not currently exercised in the helpers themselves. ^[ambiguous]
- Responses use `ResponseTemplate::new(<status>).set_body_json(<value>)`; Telegraph additionally calls `.insert_header("content-type", "application/json")` and toggles 200 vs 400 based on the modeled `SmtpResponse.code`.

## Isolation Semantics

- The shared singleton means **two parallel tests will see each other's mocks** unless they are serialized. The Hive and Telegraph suites both run with `#[serial]` (see `[[concepts/integration-testing-with-real-infrastructure]]`), and `MockServerFixture::new()` resets state between fixtures.
- The drop-time reset is best-effort: it calls `tokio::runtime::Handle::try_current()` and only spawns the reset task if a runtime handle is available. Tests that drop the fixture outside of a Tokio context will not get the post-drop cleanup. ^[inferred]
- The hard-coded `127.0.0.1:3000` listener means **only one test process can hold the wiremock fixture at a time on a given host**. Parallel cargo test workers across crates that all need this fixture will fight for the port. ^[inferred]
- `register_cleanup_handler` only logs; it does not flush mocks on Ctrl+C. The wiremock server is dropped along with the process. ^[inferred]

## When To Use This vs A Real Container

- Pick `MockServerFixture` when the dependency is an HTTP-shaped collaborator you control entirely (Hive's external provider, Telegraph's SMTP-as-HTTP shim) and when you want assertion-grade control over request/response bodies.
- Pick a real testcontainer when the test needs to verify protocol-level behavior, message persistence, or another service's parsing rules.
- The two coexist in Telegraph's tree: `SmtpService` (wiremock) and `TestSmtp` (MailHog testcontainer) are both present and chosen per test. ^[inferred]

## Open Questions

- The shared port-3000 binding is convenient locally but blocks parallel `cargo test -p ...` runs across crates that both need the fixture. There's no documented escape hatch (e.g., env-var override). ^[inferred]
- `register_cleanup_handler` mixes a `ctrlc` handler that calls `std::process::exit(0)` with a libc `atexit` callback that only logs. The intent of the `ctrlc` exit is unclear given that the comment says cleanup happens automatically anyway. ^[ambiguous]
- The drop-time async reset is fire-and-forget; nothing waits for it to complete before the next test starts. If two `MockServerFixture::new()` calls happen back-to-back, the new fixture's eager `reset_all_mocks()` is what actually guarantees isolation, not the drop. ^[inferred]

## See Also

- [[skills/stubbing-http-with-wiremock]] — how-to recipe for adding a new wiremock-backed fixture.
- [[projects/rustycog/references/rustycog-testing]] — testing module reference.
- [[projects/rustycog/references/openfga-mock-service]] — in-crate consumer for OpenFGA `Check`.
- [[projects/rustycog/references/openfga-mock-service]] — in-framework consumer for OpenFGA `Check`.
- [[concepts/integration-testing-with-real-infrastructure]] — surrounding test-strategy context.
