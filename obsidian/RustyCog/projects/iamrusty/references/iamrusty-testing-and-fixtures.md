---
title: IAMRusty Testing and Fixtures
category: references
tags: [reference, testing, fixtures, visibility/internal]
sources:
  - IAMRusty/docs/TESTING_GUIDE.md
  - IAMRusty/docs/FIXTURES_GUIDE.md
  - IAMRusty/docs/KAFKA_EVENT_TESTING_GUIDE.md
  - IAMRusty/tests/fixtures/db/mod.rs
  - IAMRusty/tests/signup_kafka.rs
summary: IAMRusty-specific testing notes layered on top of RustyCog's shared harness, focusing on auth fixtures, provider mocks, and optional queue or Kafka-backed validation.
provenance:
  extracted: 0.84
  inferred: 0.1
  ambiguous: 0.06
created: 2026-04-14T17:46:37.6929647Z
updated: 2026-04-23T19:10:00Z
---

# IAMRusty Testing and Fixtures

This page narrows `[[projects/rustycog/references/rustycog-testing]]` to the way `[[projects/iamrusty/iamrusty]]` actually validates auth behavior, provider flows, and optional event publication.

## RustyCog Baseline

- `[[projects/rustycog/references/rustycog-testing]]` explains the shared test server, migration hooks, JWT helpers, and base fixture model that IAMRusty reuses.
- `[[concepts/integration-testing-with-real-infrastructure]]` captures the broader real-infrastructure testing pattern that IAMRusty applies to auth flows.

## Service-Specific Differences

- Integration tests are built around a shared server/database fixture plus `#[serial]` execution so cleanup, state, and runtime setup stay deterministic.
- `DbFixtures` exposes fluent builders for users, emails, provider tokens, refresh tokens, verification records, and password-reset tokens, along with higher-level helpers for common auth scenarios.
- GitHub and GitLab service fixtures mock external OAuth APIs while still letting the service execute real HTTP handlers and persistence logic.
- Kafka validation exists as a real container-backed test that consumes published events, but it is intentionally `#[ignore]` because of Docker, startup time, and environment requirements.
- The Kafka test also confirms that queue and event behavior are wired through the same config-driven runtime used by the service instead of through a special test-only code path.
- Shared harness behavior now largely follows `[[projects/rustycog/references/rustycog-testing]]`, while IAMRusty keeps service-specific fixtures and auth-flow assertions.
- The testing docs still reference some local utilities that have since moved into `rustycog-testing`, so parts of the published guide lag behind the current fixture module layout. ^[ambiguous]
- The test harness compiles `iam-infra` with the [[concepts/test-only-cargo-feature-relaxation|`test-relaxed-jwt`]] Cargo feature on (activated via `IAMRusty/Cargo.toml`'s dev-dep entry), which lifts the RS256-only constructor checks in `RegistrationTokenServiceImpl` and `JwtTokenService` so the in-tree HS256 `test.toml` boots the suite without committing RSA PEM material. See [[projects/iamrusty/concepts/jwt-algorithm-enforcement-and-test-relaxation]] for the production guards this relaxes.
- The `tests/utils/jwt.rs` helpers no longer duplicate the RS256 check — they pass whatever algorithm the test config produced through to the (now relaxed) production constructor, which is the single authoritative gate.
- Tests asserting JWT header `alg` (e.g. `tests/auth_username_flow_part2.rs::test_registration_token_has_correct_rsa_signature`) accept either `RS*` or `HS*`, because the production-only RS256 invariant is not expressible while the feature is on.

## Open Questions

- Queue-backed coverage is present but incomplete, because Kafka tests are optional and the default test queue config is disabled. ^[ambiguous]
- The missing `docs/TEST_DATABASE_GUIDE.md` means part of the intended testing narrative is absent from the current repo snapshot. ^[ambiguous]

## Sources

- [[projects/iamrusty/iamrusty]] - Service whose flows are under test.
- [[concepts/integration-testing-with-real-infrastructure]] - Distilled testing concept from these sources.
- [[concepts/test-only-cargo-feature-relaxation]] - Generic Cargo-feature pattern the test harness uses to lift IAMRusty's RS256 production guards.
- [[projects/iamrusty/concepts/jwt-algorithm-enforcement-and-test-relaxation]] - IAMRusty-specific application of the pattern.
- [[projects/iamrusty/skills/testing-rust-services-with-fixtures]] - Actionable workflow built from the same patterns.
- [[projects/iamrusty/references/iamrusty-runtime-and-security]] - Config and queue context behind test setup.
- [[projects/rustycog/references/rustycog-testing]] - Shared test-runtime layer IAMRusty now builds on.
