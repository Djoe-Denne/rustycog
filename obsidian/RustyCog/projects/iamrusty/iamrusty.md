---
title: IAMRusty
category: project
tags: [iam, oauth, security, visibility/internal]
sources:
  - IAMRusty/README.md
  - IAMRusty/docs/ARCHITECTURE.md
  - IAMRusty/docs/API_REFERENCE.md
  - IAMRusty/domain/src/entity/events.rs
  - IAMRusty/setup/src/app.rs
  - IAMRusty/http/src/lib.rs
summary: IAMRusty is a Rust IAM service whose docs now treat RustyCog as the shared baseline and focus here on the auth, OAuth, JWT, and event behaviors unique to IAMRusty.
provenance:
  extracted: 0.74
  inferred: 0.18
  ambiguous: 0.08
created: 2026-04-14T17:46:37.6929647Z
updated: 2026-04-19T12:08:26.9393504Z
---

# IAMRusty

## Indexes

- [[projects/iamrusty/concepts/index]] — concepts
- [[projects/iamrusty/skills/index]] — skills
- [[projects/iamrusty/references/index]] — references

`IAMRusty` is the identity service in the AIForAll workspace. Use `[[projects/rustycog/references/index]]` for the shared service shell and crate behavior; use this page and the linked IAMRusty references for the auth, OAuth, JWT, and event-contract choices that specialize that baseline.

## RustyCog Baseline

- `[[projects/rustycog/references/index]]` is the canonical map for the shared command, config, HTTP, permission, event, DB, and testing crates used by IAMRusty.
- `[[references/rustycog-service-construction]]` and `[[skills/building-rustycog-services]]` describe the generic RustyCog service assembly flow that IAMRusty reuses.
- Read `[[projects/rustycog/references/rustycog-command]]`, `[[projects/rustycog/references/rustycog-config]]`, `[[projects/rustycog/references/rustycog-http]]`, `[[projects/rustycog/references/rustycog-events]]`, and `[[projects/rustycog/references/rustycog-testing]]` for the shared runtime mechanics this service builds on.

## Service-Specific Differences

- Authentication is intentionally dual-mode: unauthenticated users enter OAuth login, while authenticated users can attach extra providers through `[[projects/iamrusty/concepts/oauth-provider-linking]]`.
- JWT cryptography is RS256-only in production for both access tokens and registration tokens, but the strict constructor checks are gated by a Cargo feature so the in-tree HS256 `test.toml` boots the suite — see `[[projects/iamrusty/concepts/jwt-algorithm-enforcement-and-test-relaxation]]`.
- Runtime behavior depends on `[[concepts/structured-service-configuration]]`, including environment-specific TOML files, cached random ports in tests, queue settings, and JWT secret resolution.
- Command orchestration is centralized through `[[concepts/command-registry-and-retry-policies]]`, but IAMRusty is the clearest example where `CommandConfig`-driven retry policy is wired all the way into the live registry.
- The service relies on `[[concepts/integration-testing-with-real-infrastructure]]` for end-to-end confidence, using real databases, HTTP servers, fixtures, provider mocks, and optional queue-backed checks.
- IAMRusty uses `iam-events` as its domain-event contract surface, while `[[projects/rustycog/references/rustycog-events]]` provides the queue transport and publisher runtime.
- The published API and the current implementation are close but not identical: the docs still describe some older route names and payload shapes, while the live route table in `http/src/lib.rs` exposes separate login, link, and relink endpoints. ^[ambiguous]

## Related

- [[projects/rustycog/references/index]] - Canonical shared framework map that the service pages below build on.
- [[projects/iamrusty/references/iamrusty-service]] - Code-backed overview of the crate layout, route surface, and runtime wiring.
- [[projects/iamrusty/references/iamrusty-entity-model]] - Identity-side entities such as users, emails, provider links, and token artifacts.
- [[projects/iamrusty/references/iamrusty-runtime-and-security]] - Configuration, JWT, queue, TLS, and OAuth hardening details.
- [[projects/iamrusty/references/iamrusty-api-and-auth-flows]] - Public and authenticated HTTP flows, including registration completion and password reset.
- [[projects/iamrusty/references/iamrusty-command-execution]] - How the command registry wraps the service's use cases.
- [[projects/iamrusty/references/iamrusty-testing-and-fixtures]] - Test server, database fixture, and Kafka-backed validation patterns.
- [[projects/iamrusty/skills/testing-rust-services-with-fixtures]] - Preferred workflow for building IAM-style integration tests.
- [[projects/iamrusty/skills/extending-iamrusty-with-oauth-providers]] - End-to-end checklist for adding another provider safely.

## Open Questions

- The docs describe some security and deployment behavior more strongly than the current code exposes, especially around OAuth state expiry and callback URI handling. ^[ambiguous]
- `README.md` and testing docs still reference `docs/TEST_DATABASE_GUIDE.md`, but that file is not present in the current source set. ^[ambiguous]

## Sources

- [[projects/rustycog/references/index]]
- [[projects/iamrusty/references/iamrusty-service]]
- [[projects/iamrusty/references/iamrusty-runtime-and-security]]
- [[projects/iamrusty/references/iamrusty-api-and-auth-flows]]
- [[projects/iamrusty/references/iamrusty-command-execution]]
- [[projects/iamrusty/references/iamrusty-testing-and-fixtures]]
