---
title: OAuth State and CSRF Protection
category: concepts
tags: [security, oauth, csrf, visibility/internal]
sources:
  - IAMRusty/docs/OAUTH_SECURITY_GUIDE.md
  - IAMRusty/http/src/oauth_state.rs
  - IAMRusty/http/src/handlers/auth.rs
summary: IAMRusty uses encoded OAuth state and context-aware callbacks to separate login from linking, though the docs describe stronger expiry semantics than the current struct exposes.
provenance:
  extracted: 0.7
  inferred: 0.16
  ambiguous: 0.14
created: 2026-04-14T17:46:37.6929647Z
updated: 2026-04-14T17:46:37.6929647Z
---

# OAuth State and CSRF Protection

The IAM docs treat OAuth state as a core security control, not just a callback parameter. The live implementation still uses state to separate operations and reject malformed callbacks, but some of the stronger guarantees described in the guides are only partially visible in the current code.

## Key Ideas

- Login and link flows both create an `OAuthState` before redirecting to the provider, and the callback handler decodes that state before executing any auth logic.
- The current `http/src/oauth_state.rs` struct stores the operation plus a random nonce, and the link variant carries the authenticated user ID inside the operation payload.
- The callback handler rejects missing code, missing state, provider error responses, invalid providers, and invalid state before dispatching login or link commands.
- Authenticated link starts also verify the current user through `GetUserCommand`, so the link flow is tied to both bearer auth and the state payload.
- The security guide describes timestamp-based expiry and exact redirect validation, but the current `OAuthState` struct does not store a timestamp and the callback handler currently hardcodes local redirect URIs for provider callbacks. ^[ambiguous]
- OAuth failures are surfaced through dedicated `AuthError` responses, which keeps state-validation and provider errors distinct from generic API failures.

## Open Questions

- If callback URIs remain hardcoded to the local test port in handler code, production redirect handling likely depends on additional wiring or pending refactoring. ^[ambiguous]
- The docs emphasize 30-minute state expiry and richer tamper detection, but those checks are not explicit in the current state type. ^[ambiguous]

## Sources

- [[projects/iamrusty/iamrusty]] - Service where the state model is enforced.
- [[projects/iamrusty/concepts/oauth-provider-linking]] - Flow that depends on operation-aware state.
- [[projects/iamrusty/references/iamrusty-runtime-and-security]] - Security and config context for callback hardening.
- [[projects/iamrusty/references/iamrusty-api-and-auth-flows]] - Handler-level view of callback validation and error responses.
