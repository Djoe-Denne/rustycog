---
title: PermissionChecker
category: entity
tags: [entity, authorization, rustycog, openfga]
summary: >-
  Engine-neutral async trait in rustycog-permission. Every HTTP handler goes through a shared Arc<dyn PermissionChecker> before touching domain code. The production implementation calls OpenFGA.
updated: 2026-04-20
---

# PermissionChecker

`rustycog_permission::PermissionChecker` is the single interface every service uses to ask "can this subject do this action on this resource?". It replaced the old per-service `PermissionEngine` + `PermissionsFetcher` pair.

```rust
#[async_trait]
pub trait PermissionChecker: Send + Sync {
    async fn check(
        &self,
        subject: Subject,
        action: Permission,
        resource: ResourceRef,
    ) -> Result<bool, DomainError>;
}
```

## Implementations

- `OpenFgaPermissionChecker` — production, calls OpenFGA over HTTP.
- `InMemoryPermissionChecker` — test-only.
- `CachedPermissionChecker` — decorator adding a short-TTL LRU cache.

## Composition root rule

Build exactly one `Arc<dyn PermissionChecker>` per service, cache it in `AppState`, and reuse it in every middleware and domain call. Never rebuild it per request.

## Related

- [[projects/rustycog/references/rustycog-permission]]
- [[entities/subject]]
- [[entities/resource-ref]]
- [[concepts/openfga-as-authorization-engine]]
