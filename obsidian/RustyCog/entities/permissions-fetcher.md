---
title: PermissionsFetcher
category: entities
tags: [rustycog, permissions, authorization, removed, visibility/internal]
status: removed
summary: PermissionsFetcher was the service-owned permission resolver under the Casbin-based authorization model. It has been removed in favor of the centralized OpenFGA PermissionChecker.
updated: 2026-04-20
---

# PermissionsFetcher (removed)

> [!warning] Removed
> `PermissionsFetcher` no longer exists. It was the service extension point under the Casbin-based authorization model. The replacement is [[entities/permission-checker]] backed by the centralized [[concepts/openfga-as-authorization-engine]].

## Migration

- Delete every `PermissionsFetcher` implementation in your service (`*PermissionFetcher`, `*PermissionService`).
- Delete the per-service `.conf` files under `resources/permissions/`.
- Replace the `permissions_dir` / `resource` / `with_permission_fetcher` / `with_permission` chain in route setup with a single `with_permission_on(Permission, object_type)` call.
- Inject `Arc<dyn PermissionChecker>` into `AppState` (see [[entities/route-builder]]).
- Translate the domain events your service already emits into OpenFGA tuples via the [[projects/sentinel-sync/sentinel-sync]] worker; the checker then sees the same authorization facts the old fetcher used to compute.

## Rationale

Fetchers forced each service to own the cross-cutting "who can do what" question, which made cross-service decisions (org admins over project components, for example) require synchronous cross-service queries. The centralized model keeps those facts in one Zanzibar graph.

## Historical reference

For context on the previous design see the archived [[concepts/resource-scoped-permission-fetchers]] concept page (also marked deprecated).
