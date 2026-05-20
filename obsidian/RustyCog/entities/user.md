---
title: User
category: entities
tags: [identity, users, auth, visibility/internal]
sources:
  - IAMRusty/domain/src/entity/user.rs
  - IAMRusty/domain/src/entity/user_email.rs
  - IAMRusty/domain/src/entity/provider_link.rs
summary: IAMRusty models the platform user as one account that can hold profile data, multiple emails, and multiple linked OAuth identities.
provenance:
  extracted: 0.82
  inferred: 0.10
  ambiguous: 0.08
created: 2026-04-14T20:28:20.9129598Z
updated: 2026-04-14T20:28:20.9129598Z
---

# User

The canonical platform user lives in `<!-- [[projects/iamrusty/iamrusty]] -->`. Other services, especially `<!-- [[projects/hive/hive]] -->` and `<!-- [[projects/manifesto/manifesto]] -->`, usually refer to that user through `user_id` rather than owning their own separate user entity.

## Key Ideas

- `User` is the account root and can exist in incomplete form before registration is fully finished.
- The same user can authenticate with password, with OAuth providers, or with both at once.
- `UserEmail` is stored separately from the core user record so one user can carry multiple email addresses with primary and verified flags.
- `ProviderLink` captures the binding between one platform user and one external provider identity such as GitHub or GitLab.
- In practice, IAMRusty owns the identity record, while organization, project, membership, and notification services treat `user_id` as a foreign identity reference. ^[inferred]

## Open Questions

- The wiki still does not define one operator-facing rule for when a user becomes “complete” across password and OAuth flows. ^[ambiguous]
- Some services use `user_id` heavily but do not document how much profile detail they expect IAMRusty to remain authoritative for. ^[inferred]

## Sources

- <!-- [[projects/iamrusty/references/iamrusty-entity-model]] --> - IAMRusty's full identity-side entity inventory.
- [[entities/membership]] - Organization and project membership entities that point back to the user.
- [[entities/organization]] - Organizations are owned by or joined by users.
- [[entities/project]] - Projects record creators and owners through user-linked fields.
