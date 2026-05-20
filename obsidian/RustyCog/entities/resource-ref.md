---
title: ResourceRef
category: entity
tags: [entity, authorization, rustycog, openfga]
summary: >-
  Typed object reference used in PermissionChecker calls. Combines an object_type (matching an OpenFGA type) with an object UUID.
updated: 2026-04-20
---

# ResourceRef

`rustycog_permission::ResourceRef` is the typed, OpenFGA-shaped resource identifier passed into every `check` call.

```rust
pub struct ResourceRef {
    pub object_type: &'static str,
    pub object_id: Uuid,
}
```

`object_type` must match a type defined in [openfga/model.fga](../../openfga/model.fga). Typical values:

- `"organization"`
- `"project"`
- `"component"`
- `"notification"`

## Construction

`ResourceRef` is typically built by the HTTP permission middleware from the deepest UUID in the request path and the `object_type` passed to `with_permission_on(Permission, object_type)`.

Services can also build it directly for domain-layer checks:

```rust
let resource = ResourceRef::new("project", project_id);
checker.check(subject, Permission::Write, resource).await?;
```

## Wire format

Rendered as `"{type}:{uuid}"` — the string shape OpenFGA expects in `tuple_key.object`.

## Related

- [[entities/permission-checker]]
- [[entities/subject]]
- [[entities/relation-tuple]]
- [[projects/rustycog/references/rustycog-permission]]
