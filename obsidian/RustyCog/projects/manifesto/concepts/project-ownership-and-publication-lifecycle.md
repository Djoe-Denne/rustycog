---
title: Project Ownership and Publication Lifecycle
category: concepts
tags: [projects, ownership, lifecycle, visibility/internal]
sources:
  - Manifesto/README.md
  - Manifesto/application/src/usecase/project.rs
summary: Manifesto ties project creation, ownership, visibility defaults, membership bootstrap, and publish/archive transitions into one lifecycle flow.
provenance:
  extracted: 0.79
  inferred: 0.11
  ambiguous: 0.10
created: 2026-04-14T20:25:00Z
updated: 2026-04-19T11:49:06.1450368Z
---

# Project Ownership and Publication Lifecycle

`[[projects/manifesto/manifesto]]` treats project creation as both an ownership decision and a permission bootstrap. A project can be personal or organization-owned, but in either case the creator is inserted into the membership model and the lifecycle then governs whether the project stays draft, becomes active, or is archived.

## Key Ideas

- Personal projects derive `owner_id` directly from the authenticated user, while organization projects require an explicit organization owner ID.
- New projects default to `Visibility::Private` and `DataClassification::Internal` when the request does not override those fields.
- `create_project()` persists the project, creates an owner member, and emits a `ProjectCreated` event. [[projects/sentinel-sync/sentinel-sync]] translates that event into `project:{id}#owner@user:{created_by}` (and `project:{id}#organization@organization:{owner_id}` when org-owned) so the creator immediately holds owner-level relations on the project, component, and member surfaces via OpenFGA inheritance.
- `publish_project()` validates that the project is publishable before transitioning it to `active`, while `archive_project()` transitions the lifecycle to `archived`.
- Ownership, publication, and archival all emit Manifesto domain events, so lifecycle changes are modeled as integration-relevant state transitions rather than local DB updates only.
- The README documents a broader workflow including `suspended`, while the current HTTP surface centers on publish and archive operations. Conflict to resolve. ^[ambiguous]

## Open Questions

- When should `suspended` become a first-class operator-facing state in the HTTP and wiki surface? ^[ambiguous]
- Should organization-owned project creation validate more than owner presence before the project is accepted? ^[inferred]

## Sources

- [[projects/manifesto/manifesto]] - Service overview for the project-service MVP.
- [[projects/manifesto/references/manifesto-api-and-permission-flows]] - Route and use-case behavior behind creation, publication, and archival.
- [[projects/manifesto/concepts/component-instance-permissions]] - Membership and resource bootstrap that accompanies project creation.
- [[concepts/centralized-authorization-service]] - Shared OpenFGA-backed pattern that the owner bootstrap feeds into.
- [[projects/sentinel-sync/references/event-to-tuple-mapping]] - Manifesto event-to-tuple table.
