---
title: JWT Secret Storage Abstraction
category: concepts
tags: [security, jwt, auth, visibility/internal]
sources:
  - IAMRusty/docs/JWT_CONFIGURATION_GUIDE.md
  - IAMRusty/configuration/src/lib.rs
  - IAMRusty/setup/src/app.rs
summary: IAMRusty resolves JWT signing material from configurable secret backends before building token services, registration tokens, and JWKS output.
provenance:
  extracted: 0.78
  inferred: 0.14
  ambiguous: 0.08
created: 2026-04-14T17:46:37.6929647Z
updated: 2026-04-23T19:10:00Z
---

# JWT Secret Storage Abstraction

`[[projects/iamrusty/iamrusty]]` keeps JWT cryptography separate from secret storage details. Configuration resolves raw secret material first, then the runtime builds algorithm-specific token services that can sign access tokens, refresh tokens, and registration tokens while also exposing JWKS metadata.

## Key Ideas

- `JwtConfig` resolves a `SecretStorage` value into a concrete `JwtSecret`, then converts that into either HS256 or RS256 runtime configuration.
- The current code supports plain-text HMAC secrets and PEM-backed RSA key pairs, with placeholder branches for Vault and GCP Secret Manager that are not yet implemented.
- `setup/src/app.rs` uses the resolved algorithm to create both `JwtTokenService` and `RegistrationTokenServiceImpl`, so the same cryptographic source shapes regular auth and registration-completion flows.
- Both constructors enforce RS256 in production via compile-time guards (`#[cfg(not(feature = "test-relaxed-jwt"))]` on the `if` in `RegistrationTokenServiceImpl::new` and an `assert!` in `JwtTokenService::with_refresh_expiration`); the `with_hmac` constructor on `JwtTokenService` is gated on the same feature so it does not exist in release binaries. The `iam-service` test build activates the feature via its dev-dep on `iam-infra` so the in-tree HS256 `test.toml` boots — see [[projects/iamrusty/concepts/jwt-algorithm-enforcement-and-test-relaxation]] and [[concepts/test-only-cargo-feature-relaxation]].
- The HTTP layer exposes a JWKS endpoint, which makes the public verification keys part of the service contract rather than an out-of-band deployment artifact.
- The docs and examples still use names such as `[jwt.secret_storage]`, `PlainText`, and `PemFile`, while the current code and TOML files use `[jwt.secret]`, lowercase serde tags like `pem_file`, and the field name `value`. ^[ambiguous]
- The code also defaults access tokens to 15 minutes and refresh tokens to 30 days unless config overrides them.

## Open Questions

- Vault and GCP secret backends are documented architecturally, but the current implementation returns not-implemented errors for those branches. ^[ambiguous]
- The docs often assume broader operational patterns for key rotation and zero-downtime migration than are directly visible in this repo alone. ^[ambiguous]

## Sources

- [[projects/iamrusty/iamrusty]] - Service that consumes the abstraction.
- [[projects/iamrusty/references/iamrusty-runtime-and-security]] - Runtime config, JWKS, TLS, and deployment context.
- [[projects/iamrusty/references/iamrusty-api-and-auth-flows]] - API surface that exposes token and JWKS behavior.
- [[projects/iamrusty/concepts/jwt-algorithm-enforcement-and-test-relaxation]] - Production RS256 guards and how tests relax them.
- [[concepts/test-only-cargo-feature-relaxation]] - Generic Cargo-feature pattern used to gate the constructors.
- <!-- [[concepts/structured-service-configuration]] --> - Config layer that resolves secret backends.
