---
title: >-
  JWT Algorithm Enforcement and Test Relaxation
category: concepts
tags: [security, jwt, auth, testing, feature-flags, visibility/internal]
sources:
  - IAMRusty/infra/src/token/registration_token_service.rs
  - IAMRusty/infra/src/token/jwt_encoder.rs
  - IAMRusty/infra/Cargo.toml
  - IAMRusty/Cargo.toml
  - IAMRusty/setup/src/app.rs
  - IAMRusty/config/test.toml
  - IAMRusty/tests/utils/jwt.rs
  - IAMRusty/tests/auth_username_flow_part2.rs
  - rustycog/rustycog-http/src/middleware_user_id.rs
summary: >-
  IAMRusty enforces RS256 on its registration and access-token services in production but
  compiles those guards out via a Cargo feature so the in-tree HS256 test config can boot.
provenance:
  extracted: 0.6
  inferred: 0.35
  ambiguous: 0.05
created: 2026-04-23T19:10:00Z
updated: 2026-04-23T19:10:00Z
---

# JWT Algorithm Enforcement and Test Relaxation

`[[projects/iamrusty/iamrusty]]` signs three classes of token (access,
refresh, and registration) and wants production to use RS256 for all of them
so verifiers can rely on the JWKS endpoint and never see HMAC secrets.
The current implementation enforces that with two compile-time guards in
[[projects/rustycog/references/index|iam-infra]] that are lifted in test
builds via the [[concepts/test-only-cargo-feature-relaxation|`test-relaxed-jwt`
Cargo feature]].

## What is enforced

- `RegistrationTokenServiceImpl::new` returns
  `DomainError::AuthorizationError("Registration tokens must use RSA256 algorithm for security")`
  if the supplied `JwtAlgorithm` is anything other than `RS256(...)`. The
  check has been gated `#[cfg(not(feature = "test-relaxed-jwt"))]` so it is
  present in every release build but absent in the IAMRusty test build.
- `JwtTokenService::with_refresh_expiration` (the constructor that
  `setup/src/app.rs` actually calls) `assert!`s the same invariant. Same
  feature gate. The assertion was chosen over a `Result` return to avoid
  rippling a new error type through the trait surface and the composition
  root. ^[inferred]
- `JwtTokenService::with_hmac` is wrapped in
  `#[cfg(feature = "test-relaxed-jwt")]` so it does not exist at all in a
  release binary. Calling it from production code would fail to compile —
  not just panic at runtime — which is the strongest safeguard the language
  offers for "this constructor is for tests only".

## How the relaxation is wired

`IAMRusty/Cargo.toml` activates the feature only in the dev-dependency
entry on `iam-infra`:

```toml
iam-infra = { path = "infra", features = ["test-utils", "test-relaxed-jwt"] }
```

Because Cargo unifies features across the build graph, `cargo test -p iam-service`
loads dev-deps and the feature is on for every consumer of `iam-infra`
(including `iam-setup` → `iam-infra`). `cargo build --release` does not
load dev-deps, so the strict checks are reinstated and `with_hmac` vanishes.

The mechanics, motivation, and trade-offs of this approach are documented
generically in [[concepts/test-only-cargo-feature-relaxation]].

## What that lets the test config do

`IAMRusty/config/test.toml` ships a plain HS256 secret:

```toml
[jwt.secret]
type = "plain"
value = "rustycog-test-hs256-secret"
```

With the feature on, `RegistrationTokenServiceImpl::new` accepts this, and
the encode/validate paths use `self.get_algorithm()` rather than a
hard-coded `Algorithm::RS256` so the resulting JWT header carries `alg: HS256`
and verifies against the same shared secret. No PEM material is committed
to the repo; no live RSA keypair has to be generated to run the suite.

## Architectural mismatch this does not solve

`UserIdExtractor` in `[[projects/rustycog/references/index|rustycog-http]]`
only accepts HS256 — it is hard-wired to the shared-secret path. So even
when production runs `JwtTokenService` strictly on RS256 (after someone
ships RSA keys via `[jwt.secret] type = "pem_file"`), the access-token
verifier in the HTTP middleware would no longer be able to validate them.

The current state therefore has two coexisting facts:

- Production *cannot* boot today: `JwtTokenService` and
  `RegistrationTokenServiceImpl` both refuse the HS256 secret in
  `test.toml`, and the strict assertion in `with_refresh_expiration` would
  panic during `setup_app` before the HTTP server starts. This is by
  acknowledged design — there is "absolutely nothing in production" yet.
  ^[ambiguous]
- A future production build with RS256 keys *would* boot the token
  services, but anonymous request handling via `UserIdExtractor` would
  reject the issued tokens. Tracked as Phase B in the plan: extend the
  `rustycog-http` extractor to also handle RS256, or split the two
  responsibilities behind a trait.

The feature flag deliberately stops at the test boundary; it is not a fix
for the Phase B problem.

## Implications for other code

- Test utilities (`IAMRusty/tests/utils/jwt.rs`) used to duplicate the
  RS256-only check; that duplicate has been removed because the feature
  flag puts the only authoritative check inside the production constructor.
- `iamrusty-runtime-and-security` previously asserted that "current config
  files show PEM-backed JWT keys in both default and test TOMLs" — that
  reading is no longer accurate for `test.toml`. See
  [[projects/iamrusty/references/iamrusty-runtime-and-security]] for the
  current shape.
- Tests that asserted "header.alg starts with `RS`" had to be relaxed to
  accept either `RS*` or `HS*` because the production-time-only invariant
  is not expressible in this build (e.g.
  `tests/auth_username_flow_part2.rs::test_registration_token_has_correct_rsa_signature`).

## See Also

- [[concepts/test-only-cargo-feature-relaxation]] — the generic pattern.
- [[projects/iamrusty/concepts/jwt-secret-storage-abstraction]] — the
  configuration layer that resolves the underlying secret material.
- [[projects/iamrusty/references/iamrusty-runtime-and-security]] — runtime
  context and config files.
- [[projects/iamrusty/references/iamrusty-testing-and-fixtures]] — how the
  feature is activated by the test harness.
