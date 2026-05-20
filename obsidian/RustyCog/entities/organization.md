---
title: Organization
category: entities
tags: [organizations, tenants, membership, visibility/internal]
sources:
  - Hive/domain/src/entity/organization.rs
  - Hive/domain/src/entity/organization_member.rs
  - Hive/domain/src/entity/organization_invitation.rs
summary: Hive models the organization as the tenant root for membership, invitations, role-permission assignments, and external integrations.
provenance:
  extracted: 0.81
  inferred: 0.10
  ambiguous: 0.09
created: 2026-04-14T20:28:20.9129598Z
updated: 2026-04-14T20:28:20.9129598Z
---

# Organization

The canonical organization entity lives in `<!-- [[projects/hive/hive]] -->`. It acts as the tenant root for members, invitations, permissions, external links, and sync jobs, and it can also become an owner of `[[entities/project]]` records in `<!-- [[projects/manifesto/manifesto]] -->`.

## Key Ideas

- `Organization` carries name, slug, optional description/avatar, an owning `user_id`, and JSON settings.
- `OrganizationMember` attaches users to the organization with lifecycle states such as pending, active, and suspended.
- `OrganizationInvitation` models invite tokens, expiry, acceptance, and the role-permission bundle the invited user is expected to receive.
- Hive treats organizations as more than metadata containers: they are the aggregate root for membership, permissions, external provider links, and integration-triggered sync work.
- Organization ownership overlaps with Manifesto's `OwnerType::Organization`, which means Hive's tenant model is reused by another service rather than staying internal to Hive alone. ^[inferred]

## Open Questions

- The current wiki still does not map one end-to-end story for how organization membership in Hive feeds organization-owned projects in Manifesto. ^[ambiguous]
- Hive's settings JSON carries at least visibility today, but the broader settings contract is still under-documented. ^[inferred]

## Sources

- <!-- [[projects/hive/references/hive-entity-model]] --> - Hive's broader entity inventory.
- [[entities/membership]] - Membership is the main child entity of organizations.
- [[entities/project]] - Projects can be organization-owned even though they live in Manifesto.
- <!-- [[projects/hive/concepts/organization-resource-authorization]] --> - Authorization model built on top of organizations.
