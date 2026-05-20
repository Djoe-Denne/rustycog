---
title: Telegraph Testing and SMTP Fixtures
category: references
tags: [reference, testing, fixtures, visibility/internal]
sources:
  - Telegraph/config/test.toml
  - Telegraph/tests/common.rs
  - Telegraph/tests/notification_http_endpoints_test.rs
  - Telegraph/tests/user_signup_event_test.rs
  - Telegraph/tests/user_email_verified_event_test.rs
  - Telegraph/tests/fixtures/smtp/mod.rs
  - Telegraph/tests/fixtures/smtp/service.rs
  - Telegraph/tests/fixtures/smtp/resources.rs
  - Telegraph/tests/fixtures/smtp/testcontainer.rs
summary: Telegraph-specific testing notes layered on top of RustyCog's shared harness, including the wiremock-backed SmtpService fake, the MailHog testcontainer for protocol-level checks, and the end-to-end SQS + notification flows.
provenance:
  extracted: 0.78
  inferred: 0.14
  ambiguous: 0.08
created: 2026-04-14T18:18:24.0602572Z
updated: 2026-04-22T16:20:59Z
---

# Telegraph Testing and SMTP Fixtures

This page narrows `[[projects/rustycog/references/rustycog-testing]]` to the way `[[projects/telegraph/telegraph]]` proves both its notification API and queue-driven delivery paths.

## RustyCog Baseline

- `[[projects/rustycog/references/rustycog-testing]]` explains the shared test fixture model, migration hooks, JWT helpers, and boot path that Telegraph extends.
- `[[concepts/integration-testing-with-real-infrastructure]]` captures the broader real-infrastructure testing pattern that this service applies to queues and SMTP as well as HTTP.

## Service-Specific Differences

- `TelegraphTestDescriptor` extends the shared `rustycog_testing` model with real database and SQS support, while Telegraph-specific setup adds a dedicated SMTP container for email assertions.
- `setup_test_server()` creates a `TelegraphTestFixture`, clears prior SMTP state, then boots the app through the shared RustyCog test-server path so the service is exercised with real infrastructure rather than a mocked shell.
- HTTP integration tests use real JWTs, database fixture builders, and serialized execution to validate pagination, unread filtering, and ownership enforcement for notification endpoints.
- Queue-driven tests publish `iam_events` payloads through the SQS fixture and then poll either the SMTP container or the database until the expected email or notification record appears.
- When adding a new event type or delivery mode, the most reliable test shape is still end to end: publish the real queue payload, then assert the channel-specific side effect (SMTP state, persisted notification rows, or both) instead of unit-testing the processor in isolation.
- `config/test.toml` keeps the environment dynamic but realistic: DB and SQS use `port = 0`, SMTP runs locally on `1025`, and event routing stays enabled.
- Compared with the current IAMRusty pages, Telegraph's test suite leans more heavily on SQS plus SMTP verification than on provider-mock plus Kafka-style flows. ^[ambiguous]

## SMTP Fixture Stack

Telegraph keeps **two parallel SMTP fixtures** and chooses between them per test:

- **`SmtpService`** (`Telegraph/tests/fixtures/smtp/service.rs`) wraps the shared [[projects/rustycog/references/wiremock-mock-server-fixture]] to fake SMTP as HTTP. Each SMTP verb is modeled as `POST /smtp/<verb>` against the wiremock server: `mock_greeting`, `mock_ehlo`, `mock_auth`, `mock_mail_from`, `mock_rcpt_to`, `mock_data`, `mock_quit`. ^[inferred]
- High-level scenario helpers compose those primitives: `mock_successful_email_send(expected_email)`, `mock_authenticated_email_send(auth, email)`, `mock_auth_failure(auth)`, `mock_recipient_rejection(to_email)`. A `SmtpScenarioBuilder` (returned from `mock_custom_scenario()`) lets tests assemble ad-hoc multi-step flows fluently.
- `mock_data` matches body content adaptively: it always asserts the `subject` and every recipient address are present in the request body, then adds one `body_string_contains` matcher per word from the first three significant words (>2 chars) of the expected text body so template variation does not break the stub.
- Inspection helpers `received_requests`, `verify_email_sent(subject, recipient)`, and `email_count` read from `MockServer::received_requests` so tests can assert on what the service actually sent.
- Typed `SmtpResponse`/`SmtpAuthRequest`/`SmtpEmail`/`SmtpCapabilities` DTOs in `resources.rs` provide ready-made SMTP response codes (`service_ready`, `ok`, `auth_success`, `auth_failed`, `mailbox_unavailable`, `closing`) and pre-built emails (`user_signup_welcome`, `password_reset_request`, `email_verification`).
- **`TestSmtp`** (`Telegraph/tests/fixtures/smtp/testcontainer.rs`) is the heavier path: a real MailHog container started via `testcontainers` with the test-config SMTP port mapped through to MailHog's 1025, plus the API port pinned at 8025. It exposes `get_emails`, `email_count`, `has_email(subject, recipient)`, and `clear_emails` by calling MailHog's HTTP API at `/api/v1/messages`, parsing the MIME structure into `TestEmail` values.
- `TestSmtpContainer` and the global `OnceLock<Mutex<Option<Arc<TestSmtpContainer>>>>` keep the container singleton across the suite; `cleanup_existing_smtp_container` shells out to `docker stop`/`docker rm -f telegraph_test-smtp` as a fallback to avoid leaked containers between runs.
- The choice between the two: use `SmtpService` when the test asserts on what Telegraph would send under controlled response shapes; use `TestSmtp` when the test needs a real SMTP listener and round-trip parsing of the wire-level output. Follow [[skills/stubbing-http-with-wiremock]] for the wiremock side.

## Open Questions

- The event tests rely on polling loops and second-long sleeps to wait for delivery, which is practical but slower and less explicit than an acknowledgment-oriented harness. ^[inferred]
- SMTP and SQS fixtures cover the currently wired channels, but the test suite does not yet show how future direct-send or SMS-style paths would be validated. ^[ambiguous]

## Sources

- [[projects/telegraph/telegraph]] - Service whose HTTP and event flows are under test.
- [[concepts/integration-testing-with-real-infrastructure]] - Cross-service concept view of these patterns.
- [[projects/rustycog/references/rustycog-testing]] - Shared test harness Telegraph extends with SQS and SMTP fixtures.
- [[projects/rustycog/references/wiremock-mock-server-fixture]] - Underlying mock server `SmtpService` wraps.
- [[skills/stubbing-http-with-wiremock]] - Recipe behind the `mock_*` helper convention used here.
- [[projects/telegraph/references/telegraph-http-and-notification-api]] - HTTP behaviors covered by the API tests.
- [[projects/telegraph/references/telegraph-event-processing]] - Queue behaviors covered by the event tests.
