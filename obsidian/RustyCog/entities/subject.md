---
title: Subject
category: entity
tags: [entity, authorization, rustycog]
summary: >-
  Authenticated caller, wrapping the user UUID. Passed into PermissionChecker::check and rendered as user:{uuid} on the OpenFGA wire.
updated: 2026-04-20
---

# Subject

`rustycog_permission::Subject` identifies the caller on every authorization check.

```rust
pub struct Subject { pub user_id: Uuid }
```

## Wire format

`Subject` is rendered as `user:{uuid}` — the exact shape OpenFGA expects in a `tuple_key.user` field.

## Anonymous requests

Anonymous requests never reach the checker. The middleware builder (`with_permission_on`) returns `401` before constructing a `Subject`. Routes that permit anonymous access use the optional middleware variant instead.

## Related

- [[entities/permission-checker]]
- [[entities/resource-ref]]
- [[projects/rustycog/references/rustycog-permission]]
