---
title: Component-Instance Permissions
category: concepts
tags: [permissions, components, projects, openfga, visibility/internal]
sources:
  - Manifesto/http/src/lib.rs
  - Manifesto/application/src/usecase/component.rs
  - Manifesto/application/src/usecase/member.rs
  - openfga/model.fga
  - sentinel-sync/src/translator/manifesto.rs
summary: >-
  Manifesto expresses component permissions as relations on the OpenFGA `component` type, which inherits from its parent `project`. Generic grants flow through `project`; per-instance grants attach tuples directly to `component:{id}`.
updated: 2026-04-20
---

# Component-Instance Permissions

Manifesto models components as children of projects in the OpenFGA authorization graph. The inheritance is expressed in [openfga/model.fga](../../../../openfga/model.fga):

```
type component
  relations
    define project: [project]
    define editor: [user] or admin from project
    define viewer: [user] or member from project
```

So a project `admin` automatically edits every component under the project, and a project `member` automatically views every component, without needing explicit per-component tuples.

## Event -> tuple mapping

See [[projects/sentinel-sync/references/event-to-tuple-mapping]] for the full table. The component-specific rows:

| Event                    | Tuples                                                        |
|--------------------------|---------------------------------------------------------------|
| `ProjectCreated`         | `project:{id}#organization@organization:{owner_id}` (when org-owned), `project:{id}#owner@user:{created_by}` |
| `ComponentAdded`         | `component:{component_id}#project@project:{project_id}`       |
| `ComponentRemoved`       | delete `component:{component_id}#project@project:{project_id}` |
| `PermissionGranted`      | one tuple of the matching relation on the chosen object type (`component:{id}` for `resource == "component"`) |
| `PermissionRevoked`      | delete every `{owner, admin, member, viewer}` tuple on the object |

## Route layer

Every component route in [Manifesto/http/src/lib.rs](../../../../../Manifesto/http/src/lib.rs) currently uses `with_permission_on(Permission::X, "project")` because the deepest UUID in the route path is always the project id — Manifesto models `component_type` as a string segment rather than a UUID. Per-instance component authorization uses `with_permission_on(_, "component")` whenever routes switch to `{component_id}` UUID params.

## Grant/revoke semantics

`grant_permission_specific` and `grant_permission` both emit `PermissionGrantedEvent` with a `resource` string. The Manifesto translator maps:

- `resource == "component"` -> tuple on `component:{project_id}` (generic)
- anything else -> tuple on `project:{project_id}`

Revokes emit `PermissionRevokedEvent` and the translator deletes every known relation on the target object for the user so the reverse is idempotent even without remembering the original grant.

## Sources

- [[projects/manifesto/references/manifesto-api-and-permission-flows]]
- [[projects/sentinel-sync/references/event-to-tuple-mapping]]
- [[projects/sentinel-sync/references/openfga-model]]
- [[projects/rustycog/references/rustycog-permission]]
- [[concepts/openfga-as-authorization-engine]]
