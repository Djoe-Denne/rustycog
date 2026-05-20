---
title: Membership
category: entities
tags: [membership, permissions, organizations, visibility/internal]
sources:
  - Hive/domain/src/entity/organization_member.rs
  - Hive/domain/src/entity/organization_invitation.rs
  - Manifesto/domain/src/entity/project_member.rs
summary: Membership is modeled separately from the parent organization or project so services can track status, source, and permission assignments explicitly.
provenance:
  extracted: 0.80
  inferred: 0.11
  ambiguous: 0.09
created: 2026-04-14T20:28:20.9129598Z
updated: 2026-04-19T11:49:06.1450368Z
---

# Membership

Membership is a recurring platform entity pattern rather than a one-off service detail. `Hive` models organization membership, while `[[projects/manifesto/manifesto]]` models project membership, and both attach users to a larger aggregate with explicit permission state.

## Key Ideas

- Hive's `OrganizationMember` has explicit statuses such as pending, active, and suspended, and it is often created or activated through invitation flows.
- Manifesto's `ProjectMember` is modeled more around addition/removal, grace periods, last access, and resource-scoped permissions attached to the project.
- In both services, membership is not just “user belongs to container”; it is the place where permissions, source of addition, and lifecycle state are stored.
- Hive invitations pre-package role permissions before a user joins, while Manifesto tends to bootstrap permissions directly during project creation and member grant/update flows.
- Membership depends on `[[entities/user]]` for identity and on either `[[entities/organization]]` or `[[entities/project]]` for scope.

## Open Questions

- The two services use similar words (`member`, permission assignments, lifecycle) but still do not share a single canonical membership abstraction in code. ^[ambiguous]
- The boundary between “role”, “permission”, and “membership source” is service-specific enough that the wiki should keep comparing them rather than collapsing them too aggressively. ^[inferred]

## Sources

- <!-- [[projects/hive/references/hive-entity-model]] --> - Organization-side member and invitation entities.
- [[projects/manifesto/references/manifesto-entity-model]] - Project-side member and permission entities.
- [[entities/user]] - Identity entity that memberships refer back to.
- [[concepts/centralized-authorization-service]] - OpenFGA-backed authorization model that consumes membership state via tuples written by sentinel-sync.
- [[projects/sentinel-sync/references/event-to-tuple-mapping]] - Membership events translated into OpenFGA tuples.
