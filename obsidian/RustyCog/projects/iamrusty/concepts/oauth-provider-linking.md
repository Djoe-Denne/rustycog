---
title: OAuth Provider Linking
category: concepts
tags: [iam, oauth, authentication, visibility/internal]
sources:
  - IAMRusty/README.md
  - IAMRusty/docs/OAUTH_SECURITY_GUIDE.md
  - IAMRusty/http/src/handlers/auth.rs
  - IAMRusty/domain/src/service/provider_link_service.rs
summary: IAMRusty lets authenticated users attach additional OAuth providers to one account while enforcing provider uniqueness and safe email handling.
provenance:
  extracted: 0.78
  inferred: 0.14
  ambiguous: 0.08
created: 2026-04-14T17:46:37.6929647Z
updated: 2026-04-14T17:46:37.6929647Z
---

# OAuth Provider Linking

`[[projects/iamrusty/iamrusty]]` treats external providers as attachable identities rather than separate users. A logged-in account can add GitHub or GitLab credentials to the same provider-agnostic user record, while provider tokens and secondary emails are stored explicitly for later reuse.

## Key Ideas

- The HTTP layer separates unauthenticated OAuth login from authenticated linking, even though some docs still describe the behavior as two modes of the same `/start` endpoint. ^[ambiguous]
- `OAuthState` marks whether the callback is handling login or link behavior, and link flows bind the pending operation to a specific authenticated user.
- `ProviderLinkService` verifies the target user exists, blocks linking a provider that already belongs to the same or a different user, and persists the provider token set after success.
- Provider emails are handled carefully: a new provider email becomes a secondary unverified email, while first-time link attempts fail if that email already belongs to another user.
- Relink behavior is intentionally more lenient than first-time linking, replacing tokens for an already linked provider while skipping conflicting external emails instead of failing the whole operation.
- Authenticated internal endpoints can later retrieve or revoke stored provider tokens, so linking is part of a larger provider-token lifecycle rather than a one-off login shortcut.

## Open Questions

- The current source set only covers GitHub and GitLab explicitly; extending the same guarantees to more providers depends on additional wiring and tests. ^[ambiguous]
- The user-facing conflict-resolution experience is only partially documented, especially when provider profile data and existing account data disagree. ^[ambiguous]

## Sources

- [[projects/iamrusty/iamrusty]] - Main service where linking is a first-class flow.
- [[projects/iamrusty/concepts/oauth-state-and-csrf-protection]] - State handling and callback validation that protect the flow.
- [[projects/iamrusty/references/iamrusty-api-and-auth-flows]] - Route-level behavior for login, link, relink, and provider-token endpoints.
- [[projects/iamrusty/references/iamrusty-runtime-and-security]] - Security guidance that motivates the linking constraints.
