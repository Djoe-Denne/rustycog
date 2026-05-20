---
title: Testing Rust Services with Fixtures
category: skills
tags: [testing, fixtures, rust, visibility/internal]
sources:
  - IAMRusty/docs/TESTING_GUIDE.md
  - IAMRusty/docs/FIXTURES_GUIDE.md
  - IAMRusty/tests/fixtures/db/mod.rs
  - IAMRusty/tests/signup_kafka.rs
summary: Build reliable IAMRusty-style integration tests by combining serial execution, real DB fixtures, provider mocks, and optional queue-backed checks.
provenance:
  extracted: 0.84
  inferred: 0.12
  ambiguous: 0.04
created: 2026-04-14T17:46:37.6929647Z
updated: 2026-04-14T17:46:37.6929647Z
---

# Testing Rust Services with Fixtures

This skill captures the house style behind `[[projects/iamrusty/iamrusty]]`: prefer real infrastructure for auth and integration behavior, then use focused fixtures where external providers or queue systems would otherwise make tests brittle.

## Key Ideas

- Start from `<!-- [[concepts/integration-testing-with-real-infrastructure]] -->`, not from isolated mocks, when you need confidence in routes, tokens, or persistence behavior.
- Use `setup_test_server()` and `TestFixture` as the default harness so HTTP, DB, and config are aligned for the whole test.
- Use `DbFixtures` helpers to build state quickly and consistently instead of inserting rows by hand.
- Use GitHub and GitLab service fixtures to isolate provider behavior while preserving full-stack IAM flow logic.
- Reach for Kafka-backed validation only when event publication is part of the behavior under test; keep those tests opt-in because of Docker and timing overhead.

## Workflow

- Create the test as `#[tokio::test]` plus `#[serial]` so shared infrastructure stays deterministic.
- Bootstrap the server and DB fixture first, then build the minimum provider or DB state needed for the scenario.
- Prefer high-level helpers like `create_user_with_email_password()` and `create_user_with_oauth_provider()` when they fit the case.
- Validate both the HTTP contract and the persistent side effects, not just one or the other.
- Keep Kafka tests isolated and ignored by default unless the queue side effect is the reason the test exists.

## Open Questions

- Some older docs reference local common test modules that are now partly provided by `rustycog-testing`. ^[ambiguous]

## Sources

- [[projects/iamrusty/iamrusty]] - Primary service using these patterns.
- [[projects/iamrusty/references/iamrusty-testing-and-fixtures]] - Concrete examples behind the workflow.
- <!-- [[concepts/integration-testing-with-real-infrastructure]] --> - Broader testing philosophy this skill operationalizes.
