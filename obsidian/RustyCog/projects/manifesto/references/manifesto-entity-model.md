---
title: Manifesto Entity Model
category: references
tags: [reference, entities, projects, visibility/internal]
sources:
  - Manifesto/domain/src/entity/project.rs
  - Manifesto/domain/src/entity/project_component.rs
  - Manifesto/domain/src/entity/project_member.rs
  - Manifesto/domain/src/entity/permission.rs
  - Manifesto/domain/src/entity/resource.rs
  - Manifesto/domain/src/entity/role_permission.rs
  - Manifesto/domain/src/entity/project_member_role_permission.rs
summary: Inventory of Manifesto's project, component, membership, and project-scoped RBAC entities.
provenance:
  extracted: 0.85
  inferred: 0.08
  ambiguous: 0.07
created: 2026-04-14T20:28:20.9129598Z
updated: 2026-04-19T11:49:06.1450368Z
---

# Manifesto Entity Model

This page lists the main entities `[[projects/manifesto/manifesto]]` owns in its project-service domain.

## Key Entities

- `Project` is the aggregate root and combines ownership, lifecycle, visibility, classification, and collaboration flags.
- `ProjectComponent` attaches one typed component instance to a project and tracks its own status lifecycle.
- `ProjectMember` stores user membership, source of addition, removal state, last access, and project-scoped permissions.
- `Permission`, `Resource`, `RolePermission`, and `ProjectMemberRolePermission` mirror a project-scoped RBAC model beneath the project aggregate.
- Compared with Hive, Manifesto repeats the same broad authorization pattern at project scope instead of organization scope. ^[inferred]

## Open Questions

- The current wiki still does not fully separate which Manifesto entities are stable MVP records and which are scaffolding for the broader component-service architecture described in its ADRs. ^[ambiguous]

## Sources

- [[entities/project]] - Canonical project entity page.
- [[entities/membership]] - Shared membership pattern across Manifesto and Hive.
- [[projects/manifesto/concepts/component-instance-permissions]] - How the RBAC entities and component entities interact.
- [[projects/manifesto/references/manifesto-api-and-permission-flows]] - API behavior built on top of these entities.
