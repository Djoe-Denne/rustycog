---
title: >-
  Test-Only Cargo Feature Relaxation
category: concepts
tags: [testing, rust, cargo, feature-flags, security, visibility/internal]
sources:
  - IAMRusty/infra/Cargo.toml
  - IAMRusty/infra/src/token/registration_token_service.rs
  - IAMRusty/infra/src/token/jwt_encoder.rs
  - IAMRusty/Cargo.toml
  - IAMRusty/tests/utils/jwt.rs
summary: >-
  Pattern for using a Cargo feature activated only via a parent crate's dev-dependencies
  to compile out production-only invariants in test builds without changing runtime behavior.
provenance:
  extracted: 0.55
  inferred: 0.4
  ambiguous: 0.05
created: 2026-04-23T19:10:00Z
updated: 2026-04-23T19:10:00Z
---

# Test-Only Cargo Feature Relaxation

A reusable Rust pattern for the recurring problem "this constructor must reject X
in production, but my test config can only supply X right now". Instead of
splitting the type, threading a runtime flag, or shipping fake credentials,
declare a Cargo feature on the producing crate and have the consuming crate's
**dev-dependencies** activate it. Production builds never load dev-deps, so the
strict check stays in place; test builds compile it out cleanly.

## Why a Cargo feature instead of `#[cfg(test)]`

`#[cfg(test)]` is set only when compiling *that* crate's own tests. It is
**not** propagated to dependent crates' test runs — so `crate-a` cannot use
`#[cfg(test)]` to relax a check that fires when `crate-b`'s integration tests
construct one of its types. A Cargo feature, in contrast, is part of the
build graph: when `crate-b`'s `[dev-dependencies]` enable
`crate-a/test-relaxed-x`, every consumer of `crate-a` in the test build sees
the feature active. ^[inferred]

Runtime env-vars (e.g. `IF_TEST_MODE`) work, but they:

- Add a branch to production code that has to be reasoned about and audited.
- Can be set in production by mistake or by an attacker with config access.
- Do not get the compiler's help — the code under the relaxed branch is always
  built, so it can rot silently.

Compile-time relaxation removes the relaxed code from release binaries entirely.
^[inferred]

## Mechanics

1. **Producer crate** (`crate-a/Cargo.toml`) declares the feature:

   ```toml
   [features]
   test-relaxed-x = []
   ```

2. **Producer code** gates the strict check on the *negation* of the feature so
   the default build keeps the production guard:

   ```rust
   #[cfg(not(feature = "test-relaxed-x"))]
   if !is_acceptable(&value) {
       return Err(...);
   }
   ```

3. Any *test-only* constructor (e.g. `with_hmac`, `from_string`) that should
   not exist in release binaries gates its **whole definition** on the
   feature, so a release build that calls it fails to compile rather than
   silently accepting weak input:

   ```rust
   #[cfg(feature = "test-relaxed-x")]
   pub fn with_hmac(secret: String) -> Self { ... }
   ```

4. **Consumer crate** activates the feature only in its dev-dependency entry:

   ```toml
   [dev-dependencies]
   crate-a = { path = "crate-a", features = ["test-relaxed-x"] }
   ```

5. Cargo unifies features across the build graph, so during `cargo test` the
   feature is active everywhere `crate-a` is used (including via deeper
   crates such as `crate-a-setup`); during `cargo build --release` no
   dev-deps load and the feature is off. ^[inferred]

## Consequences for the relaxed type

Once the strict constructor check is gated, code paths that previously could
assume the strict invariant must be made to handle the now-reachable variants
unconditionally — or be gated themselves. In practice:

- Helper methods (`get_encoding_key`, `get_decoding_key`) that returned
  `Err(...)` for the rejected variant should now return real values so the
  test build actually works. The production guard at the constructor still
  prevents those branches from being reached at runtime in release builds.
- Hard-coded enums in encode/validate paths (`Algorithm::RS256`) must be
  replaced by a `self.get_algorithm()` accessor so the wire output matches
  whatever key material the now-permitted variant carries.
- The test utility's own duplicate of the production guard becomes
  redundant — delete it, the production guard is the only place that should
  decide.

## When the pattern fits

- A constructor enforces a security invariant (algorithm choice, key length,
  audience claim presence) that you don't want to weaken at runtime.
- Test fixtures genuinely cannot satisfy the invariant without committing
  sensitive material (RSA private keys, real signing certificates) into the
  repo.
- The relaxed variant of the code is already implemented end-to-end — the
  feature just lifts the entry-point check; it does not paper over missing
  functionality.

## When the pattern is wrong

- The "test" path is not actually a test path but a deployment path waiting
  to leak. If `production.toml` boots happily with the relaxed variant the
  feature is masking a real bug — fix the bug or leave the strict check in
  place and ship the right credentials.
- The relaxation requires substantially different code (different
  algorithms, different storage backends). At that point you want a
  separate test-only implementation behind a trait, not a feature on the
  production type.

## See Also

- [[projects/iamrusty/concepts/jwt-algorithm-enforcement-and-test-relaxation]]
  for the canonical application of this pattern in the workspace, including
  the open architectural question that the feature flag deliberately leaves
  unresolved.
- [[concepts/integration-testing-with-real-infrastructure]] for the broader
  test-with-real-things philosophy this pattern supports.
- [[projects/iamrusty/concepts/jwt-secret-storage-abstraction]] for the
  surrounding configuration layer.
