---
title: Hive Entity Model
category: references
tags: [reference, entities, organizations, visibility/internal]
sources:
  - Hive/domain/src/entity/organization.rs
  - Hive/domain/src/entity/organization_member.rs
  - Hive/domain/src/entity/organization_invitation.rs
  - Hive/domain/src/entity/permission.rs
  - Hive/domain/src/entity/resource.rs
  - Hive/domain/src/entity/role_permission.rs
  - Hive/domain/src/entity/organization_member_role_permission.rs
  - Hive/domain/src/entity/external_provider.rs
  - Hive/domain/src/entity/external_link.rs
  - Hive/domain/src/entity/sync_job.rs
summary: Inventory of Hive's organization, membership, RBAC, and integration entities.
provenance:
  extracted: 0.85
  inferred: 0.08
  ambiguous: 0.07
created: 2026-04-14T20:28:20.9129598Z
updated: 2026-04-19T11:13:11Z
---

# Hive Entity Model

This page lists the main entities `[[projects/hive/hive]]` owns in its organization-management domain.

## Key Entities

- `Organization` is the aggregate root for tenant identity, slugging, owner linkage, and settings.
- `OrganizationMember` and `OrganizationInvitation` handle onboarding and lifecycle for users joining the organization.
- `Permission`, `Resource`, `RolePermission`, and `OrganizationMemberRolePermission` form Hive's organization-scoped RBAC model.
- `ExternalProvider` and `ExternalLink` model third-party integrations attached to an organization.
- `SyncJob` tracks long-running synchronization work and outcomes for those integrations.
- `Organization` and membership lifecycle entities align with shared platform vocabulary, while Hive-specific RBAC and integration entities extend that core model for service-local behavior.

## Open Questions

- Hive's entity model is broad enough to support a larger org platform than the currently exposed HTTP surface, so the boundary between “entity exists” and “feature is fully operable” still needs regular clarification. ^[ambiguous]

## Sources

- [[entities/organization]] - Canonical organization entity page.
- [[entities/membership]] - Membership entity pattern shared with Manifesto.
- [[projects/hive/references/hive-data-model-and-schema]] - Schema-backed view of these entities.
- [[projects/hive/concepts/organization-resource-authorization]] - Permission logic consuming the RBAC entities.
