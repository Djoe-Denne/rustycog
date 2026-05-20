---
title: Project
category: entities
tags: [projects, ownership, lifecycle, visibility/internal]
sources:
  - Manifesto/domain/src/entity/project.rs
  - Manifesto/domain/src/entity/project_component.rs
  - Manifesto/domain/src/entity/project_member.rs
summary: Manifesto models a project as the main workspace aggregate, with ownership, visibility, lifecycle state, attached components, and member access.
provenance:
  extracted: 0.83
  inferred: 0.09
  ambiguous: 0.08
created: 2026-04-14T20:28:20.9129598Z
updated: 2026-04-19T11:49:06.1450368Z
---

# Project

The canonical project entity lives in `[[projects/manifesto/manifesto]]`. A project is more than a name and status row: it is the aggregate that ties ownership, visibility, attached components, and member permissions together.

## Key Ideas

- `Project` carries lifecycle state, ownership shape, creator identity, visibility, collaboration flags, and data-classification metadata.
- Projects can be owned personally or by an organization, so they bridge `[[entities/user]]` and `[[entities/organization]]`.
- `ProjectComponent` attaches typed capabilities to a project and tracks a component status progression from pending toward active and disabled states.
- `ProjectMember` attaches users to the project with a source, lifecycle, and resource-scoped permissions rather than a single flat role string.
- Manifesto therefore uses the project as both a product-facing workspace object and the scope root for its permission model.

## Open Questions

- The current wiki still leaves some distance between the concrete project entity in code and the broader component-service orchestration envisioned in the Manifesto ADRs. ^[ambiguous]
- The exact operator story for suspended projects remains less visible than draft/active/archived transitions. ^[ambiguous]

## Sources

- [[projects/manifesto/references/manifesto-entity-model]] - Manifesto's full entity inventory.
- [[projects/manifesto/concepts/project-ownership-and-publication-lifecycle]] - Lifecycle and ownership behavior around projects.
- [[entities/membership]] - Membership model attached to projects.
- [[projects/manifesto/concepts/component-instance-permissions]] - Component and permission behavior living under the project aggregate.
