---
title: IAMRusty Runtime and Security
category: references
tags: [reference, configuration, security, visibility/internal]
sources:
  - IAMRusty/docs/DATABASE_CONFIGURATION.md
  - IAMRusty/docs/JWT_CONFIGURATION_GUIDE.md
  - IAMRusty/docs/OAUTH_SECURITY_GUIDE.md
  - IAMRusty/docs/KAFKA_INTEGRATION.md
  - IAMRusty/docs/DEPLOYMENT_GUIDE.md
  - IAMRusty/configuration/src/lib.rs
  - IAMRusty/config/default.toml
  - IAMRusty/config/test.toml
  - IAMRusty/http/src/oauth_state.rs
  - IAMRusty/setup/src/app.rs
summary: IAMRusty-specific runtime and security notes layered on top of RustyCog's shared config and event model, especially around JWTs, OAuth state, and queue-versus-Kafka drift.
provenance:
  extracted: 0.72
  inferred: 0.15
  ambiguous: 0.13
created: 2026-04-14T17:46:37.6929647Z
updated: 2026-04-23T19:10:00Z
---

# IAMRusty Runtime and Security

This page narrows `[[projects/rustycog/references/rustycog-config]]` and `[[projects/rustycog/references/rustycog-events]]` to the runtime and security behavior that is specific to `[[projects/iamrusty/iamrusty]]`.

## RustyCog Baseline

- `[[projects/rustycog/references/rustycog-config]]` explains the shared typed loader, env-prefix behavior, and queue-config primitives that IAMRusty reuses.
- `[[projects/rustycog/references/rustycog-events]]` explains the shared queue publisher runtime that now sits underneath IAMRusty's event publication.
- `[[concepts/structured-service-configuration]]` captures the cross-service config pattern that IAMRusty specializes for security-sensitive runtime settings.

## Service-Specific Differences

- The real config loader uses the `IAM` prefix and one typed `AppConfig` that includes server, database, OAuth, JWT, logging, command, queue, and legacy Kafka sections.
- Structured database config supports nested credentials, read replicas, and cached random ports, which are especially important in the test environment.
- JWT behavior is driven by resolved secret storage, not by hardcoded algorithms: the runtime can build HS256 or RS256 token services and surface public verification data through JWKS.
- `default.toml` is RS256 via `[jwt.secret] type = "pem_file"`, while `config/test.toml` ships a plain HS256 secret (`[jwt.secret] type = "plain"`). The RS256-only constructor checks in `RegistrationTokenServiceImpl::new` and `JwtTokenService::with_refresh_expiration` are compiled out for the test build via the [[concepts/test-only-cargo-feature-relaxation|`test-relaxed-jwt`]] Cargo feature; see [[projects/iamrusty/concepts/jwt-algorithm-enforcement-and-test-relaxation]] for the full story.
- The HS256 test config means the service today cannot boot from `production.toml` either — the strict assertion in `with_refresh_expiration` would panic during `setup_app` because no production RSA keys are configured. This is by design; production deployment is not yet started. ^[ambiguous]
- A separate architectural mismatch is open: `rustycog-http`'s `UserIdExtractor` only accepts HS256, so a future RS256 production deployment would break access-token verification at the HTTP middleware layer. Tracked as Phase B in the JWT relaxation plan. ^[ambiguous]
- The runtime now builds queue-backed publishers from `config.queue`, but docs still discuss local Kafka configuration and legacy Kafka-specific entry points alongside that newer queue abstraction. ^[ambiguous]
- The OAuth security guide describes timestamped, expiring state and exact redirect validation, while the current `http/src/oauth_state.rs` only stores operation plus nonce and the callback handler hardcodes local redirect URIs. ^[ambiguous]

## Open Questions

- Operator-facing docs mix `APP_` and `IAM_` env prefixes, so deployment instructions and live config behavior are not fully aligned. ^[ambiguous]
- The docs often show `[jwt.secret_storage]` examples, but the actual config files and loader use `[jwt.secret]` plus lowercase serde tags like `pem_file`. ^[ambiguous]

## Sources

- [[projects/iamrusty/iamrusty]] - Service whose runtime is being configured.
- [[concepts/structured-service-configuration]] - Main concept distilled from these sources.
- [[projects/iamrusty/concepts/jwt-secret-storage-abstraction]] - JWT-specific secret-resolution pattern.
- [[projects/iamrusty/concepts/jwt-algorithm-enforcement-and-test-relaxation]] - RS256 production guards and the `test-relaxed-jwt` feature flag.
- [[projects/iamrusty/concepts/oauth-state-and-csrf-protection]] - OAuth hardening behavior and implementation drift.
- [[concepts/test-only-cargo-feature-relaxation]] - Generic Cargo-feature pattern reused for the JWT guards above.
- [[projects/rustycog/references/rustycog-config]] - Shared configuration/runtime primitives reused by IAMRusty.
- [[projects/rustycog/references/rustycog-events]] - Queue transport runtime paired with IAM-specific event contracts.
