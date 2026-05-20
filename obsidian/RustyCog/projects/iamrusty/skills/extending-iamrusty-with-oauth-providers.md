---
title: Extending IAMRusty with OAuth Providers
category: skills
tags: [oauth, services, rust, visibility/internal]
sources:
  - IAMRusty/docs/PROVIDER_FACTORY_GUIDE.md
  - IAMRusty/setup/src/app.rs
  - IAMRusty/http/src/handlers/auth.rs
summary: Add an OAuth provider to IAMRusty by updating provider enums, infra clients, setup wiring, route validation, config, and tests together.
provenance:
  extracted: 0.76
  inferred: 0.18
  ambiguous: 0.06
created: 2026-04-14T17:46:37.6929647Z
updated: 2026-04-14T17:46:37.6929647Z
---

# Extending IAMRusty with OAuth Providers

Adding a provider to `[[projects/iamrusty/iamrusty]]` is a cross-cutting change. The docs describe a provider-factory style extension path, while the current runtime wiring also requires direct updates in setup, validation, and tests.

## Key Ideas

- Start from the provider enum and string mappings so the new provider can participate in domain logic and route parsing.
- Implement the provider client in infrastructure, normalizing provider-specific tokens and user profile data into the shared IAM abstractions.
- Register the provider anywhere the runtime creates OAuth services: login, link, relink, and internal provider-token flows all need consistent wiring in `setup/src/app.rs`.
- Update HTTP validation and handler parsing so the route layer accepts the new provider name instead of rejecting it before any use case runs.
- Extend config, redirect URIs, and tests at the same time so the provider exists in development, test, and production shapes instead of only in the domain layer.
- The provider-factory guide is useful conceptually, but the current codebase still performs several registrations directly in setup and handler logic rather than through one universal factory. ^[ambiguous]

## Workflow

- Add the provider enum variant and string conversions in the domain model.
- Create the provider OAuth client in `infra/src/auth/` and normalize remote profile data to the shared provider profile type.
- Register the client in every relevant OAuth service instance built in `setup/src/app.rs`.
- Update handler validation and provider parsing in `http/src/handlers/auth.rs`.
- Add config values, mock fixtures, and integration tests for login, callback, linking, and relinking behavior.

## Open Questions

- The current `ProviderPath` validator and route handlers are explicitly GitHub/GitLab-only, so provider expansion is not yet a purely data-driven operation. ^[ambiguous]

## Sources

- [[projects/iamrusty/iamrusty]] - Service being extended.
- [[projects/iamrusty/concepts/oauth-provider-linking]] - Existing provider-link semantics the new provider must preserve.
- <!-- [[concepts/structured-service-configuration]] --> - Config model the provider must plug into.
- [[projects/iamrusty/references/iamrusty-api-and-auth-flows]] - Route and handler behavior affected by provider expansion.
