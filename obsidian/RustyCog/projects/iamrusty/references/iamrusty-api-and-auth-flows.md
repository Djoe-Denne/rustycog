---
title: IAMRusty API and Auth Flows
category: references
tags: [reference, api, oauth, visibility/internal]
sources:
  - IAMRusty/docs/API_REFERENCE.md
  - IAMRusty/docs/EMAIL_PASSWORD_AUTH_GUIDE.md
  - IAMRusty/docs/ERROR_HANDLING_GUIDE.md
  - IAMRusty/docs/INPUT_VALIDATION_GUIDE.md
  - IAMRusty/http/src/lib.rs
  - IAMRusty/http/src/handlers/auth.rs
  - IAMRusty/domain/src/service/oauth_service.rs
  - IAMRusty/domain/src/service/auth_service.rs
  - IAMRusty/application/src/usecase/password_reset.rs
summary: Source-backed view of IAMRusty's route table, validated handler contracts, incomplete-registration flows, and the biggest API doc-code mismatches.
provenance:
  extracted: 0.69
  inferred: 0.14
  ambiguous: 0.17
created: 2026-04-14T17:46:37.6929647Z
updated: 2026-04-14T17:46:37.6929647Z
---

# IAMRusty API and Auth Flows

These sources describe the current HTTP behavior of `[[projects/iamrusty/iamrusty]]`, including public auth, authenticated profile and provider operations, registration completion, and password reset.

## Key Ideas

- The live route table exposes public signup, login, verify, resend-verification, complete-registration, username-check, password-reset, OAuth login, OAuth callback, token refresh, and JWKS behavior.
- Authenticated routes add `/api/me`, authenticated password reset, provider-token retrieval and revoke, provider link, and provider relink behavior.
- The handler layer relies on `axum_valid` for path, query, and JSON validation, then dispatches typed commands with metadata-rich `CommandContext` values.
- OAuth login callbacks hand off provider results into `domain/src/service/oauth_service.rs`, where provider identities are resolved into existing users or new incomplete registrations.
- Email/password signup and OAuth can both yield incomplete users who must finish registration later with a registration token and a chosen username.
- Resend verification and password-reset request flows intentionally return success-style responses even when the email does not exist, reducing user-enumeration leaks.
- The docs and code disagree in several high-signal places: docs often describe `/api/auth/{provider}/start`, `POST /api/auth/verify`, and signup requests that include username, while the current code exposes separate `/login` and `/link` starts, `GET /api/auth/verify` with query parameters, and two-step registration completion. ^[ambiguous]

## Open Questions

- The OAuth callback handler currently hardcodes `http://127.0.0.1:8081/...` redirect URIs for provider callbacks, which looks test-oriented rather than like the final production behavior. ^[ambiguous]
- Validation docs emphasize 422 behavior, while some API docs still describe 400 for malformed input; the external contract should be treated as partially drifted until confirmed end to end. ^[ambiguous]

## Sources

- [[projects/iamrusty/iamrusty]] - Service exposing these routes.
- [[projects/iamrusty/concepts/oauth-provider-linking]] - Link and relink semantics in the authenticated flow.
- [[projects/iamrusty/concepts/oauth-state-and-csrf-protection]] - Callback validation and state handling.
- [[projects/iamrusty/references/iamrusty-service]] - Route-table and runtime composition context.
