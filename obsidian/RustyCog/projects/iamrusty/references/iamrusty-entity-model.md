---
title: IAMRusty Entity Model
category: references
tags: [reference, entities, iam, visibility/internal]
sources:
  - IAMRusty/domain/src/entity/user.rs
  - IAMRusty/domain/src/entity/user_email.rs
  - IAMRusty/domain/src/entity/provider.rs
  - IAMRusty/domain/src/entity/provider_link.rs
  - IAMRusty/domain/src/entity/password_reset_token.rs
  - IAMRusty/domain/src/entity/email_verification.rs
  - IAMRusty/domain/src/entity/registration_token.rs
  - IAMRusty/domain/src/entity/token.rs
summary: Inventory of IAMRusty's identity-side entities, from the canonical user record to email, provider, verification, and token artifacts.
provenance:
  extracted: 0.86
  inferred: 0.08
  ambiguous: 0.06
created: 2026-04-14T20:28:20.9129598Z
updated: 2026-04-19T11:13:11Z
---

# IAMRusty Entity Model

This page lists the main entities `[[projects/iamrusty/iamrusty]]` deals with on the identity side.

## Key Entities

- `User` is the root account entity and can exist in incomplete or fully registered form.
- `UserEmail` stores one email per row so a user can hold multiple addresses with primary and verified flags.
- `Provider` and `ProviderLink` model external OAuth providers and their binding back to the platform user.
- `PasswordResetToken`, `EmailVerification`, and registration-token payloads model short-lived auth and account-completion flows.
- JWT and JWK-related structs live in the token entity layer because IAMRusty owns the token and verification surface, even though some of them are runtime/auth artifacts rather than long-lived business records.
- Durable identity records (`User`, `UserEmail`, `Provider`, `ProviderLink`) should be reasoned about separately from ephemeral auth artifacts (verification/reset/registration tokens and JWT/JWK structs) when discussing ownership and retention.

## Open Questions

- The line between durable identity entities and auth/session artifacts is useful in IAMRusty, but the current wiki still groups several token types together more loosely than the code does. ^[ambiguous]

## Sources

- [[entities/user]] - Canonical cross-service user entity.
- [[projects/iamrusty/references/iamrusty-service]] - Broader service overview around these entities.
- [[projects/iamrusty/concepts/oauth-provider-linking]] - Provider-link semantics built on top of the entity model.
