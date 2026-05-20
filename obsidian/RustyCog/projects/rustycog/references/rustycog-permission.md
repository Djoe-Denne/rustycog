---
title: RustyCog Permission
category: references
tags: [reference, rustycog, permissions, openfga, visibility/internal]
sources:
  - rustycog/rustycog-permission/src/lib.rs
  - rustycog/rustycog-permission/src/checker.rs
  - rustycog/rustycog-config/src/lib.rs
  - openfga/model.fga
summary: >-
  rustycog::permission defines PermissionChecker plus OpenFGA, in-memory, cache, and metrics implementations; shared OpenFGA config lives in rustycog::config.
provenance:
  extracted: 0.9
  inferred: 0.07
  ambiguous: 0.03
created: 2026-04-15
updated: 2026-05-20T14:05:00Z
---

# RustyCog Permission

`rustycog::permission` (historically `rustycog-permission`) is the authorization client used by every RustyCog service. It exposes permission primitives and a checker trait; the production implementation issues `Check` calls against the centralized [[concepts/openfga-as-authorization-engine]] deployment.

## Surface

- `Permission` — `Read`, `Write`, `Admin`, `Owner`. Maps to OpenFGA relations (`read`, `write`, `administer`, `own`) via `Permission::relation()`.
- `Subject` — caller identity. Carries `user_id: Uuid` plus a `kind: SubjectKind` discriminant (`User` or `Wildcard`). `Subject::new(uid)` builds the user form (renders as `user:{uuid}`); `Subject::wildcard()` builds the anonymous "any user" form (renders as `user:*`). The wildcard variant is the one [[concepts/anonymous-public-read-via-wildcard-subject]] relies on.
- `SubjectKind` — `enum { User, Wildcard }`. `#[serde(default)]` on `Subject.kind` keeps wire compatibility with payloads serialized before the field existed.
- `ResourceRef` — `{ object_type, object_id }`. `object_type` must match a type in [openfga/model.fga](../../../../openfga/model.fga).
- `ResourceId` — the legacy UUID-only resource wrapper, kept for middleware path extraction.
- `PermissionChecker` — async trait `check(subject, action, resource) -> Result<bool, DomainError>`.

## Implementations

- `OpenFgaPermissionChecker` — production. Built from `OpenFgaClientConfig` from [[projects/rustycog/references/rustycog-config]] (`scheme`, `host`, `port`, `store_id`, optional `authorization_model_id`, optional `api_token`, optional `cache_ttl_seconds`). POSTs to `{api_url()}/stores/{id}/check`. Forwards `subject.to_string()` verbatim, so the wildcard form (`user:*`) Just Works on the wire.
- `InMemoryPermissionChecker` — deterministic, test-only. `allow` / `deny` mutate an internal set of tuples. Accepts `Subject::wildcard()` like any other subject.
- `CachedPermissionChecker` — decorates any inner `Arc<dyn PermissionChecker>` with a `moka` LRU cache keyed by `(user_id, permission, object_type, object_id)`. Time-based invalidation only. **Bypasses the cache entirely when `subject.is_wildcard()`** — the cache key would collide across all anonymous requests (every wildcard reuses `Uuid::nil()`) and a public→private flip needs to be visible on the next request, not after the TTL window.
- `MetricsPermissionChecker` — `tracing`-instrumented decorator emitting structured `permission decision` events for every check, including `decision="allow"|"deny"` and `elapsed_us`.

## Wiring

The checker is constructed once in each service's composition root and injected into `AppState` so HTTP middleware (`with_permission_on`) can share it.

```rust
let raw = Arc::new(OpenFgaPermissionChecker::new(cfg.openfga.clone())?);
let cache_ttl = cfg.openfga.cache_ttl_seconds.unwrap_or(15);
let inner: Arc<dyn PermissionChecker> = if cache_ttl == 0 {
    raw
} else {
    Arc::new(CachedPermissionChecker::new(raw, Duration::from_secs(cache_ttl), 10_000))
};
let checker: Arc<dyn PermissionChecker> = Arc::new(MetricsPermissionChecker::new(inner));
```

`OpenFgaClientConfig` is owned by `rustycog::config` and re-exported from `rustycog::permission` for compatibility. The split `scheme` / `host` / `port` fields mirror DB/SQS config so tests can set `port = 0`; `actual_port()` resolves that into a cached random host port for [[projects/rustycog/references/openfga-real-testcontainer-fixture]].

`cache_ttl_seconds` (added 2026-04-22) makes the cache decoration opt-out:

- `None` (default) — production behavior, 15s TTL.
- `Some(n)` for `n > 0` — explicit TTL override.
- `Some(0)` — skip the cache entirely. **Required** in test configs that use a mutable OpenFGA fixture and need to flip a `Check` decision mid-test (e.g. grant ➜ revoke ➜ deny scenarios). Without it, `CachedPermissionChecker` would serve a stale allow from the first request and the second decision would never reach OpenFGA.

## Linked Entities

- [[entities/permission-checker]]
- [[entities/subject]]
- [[entities/resource-ref]]
- [[entities/resource-id]]

## Related

- [[concepts/openfga-as-authorization-engine]]
- [[concepts/zanzibar-relation-tuples]]
- [[concepts/anonymous-public-read-via-wildcard-subject]] — `Subject::wildcard()` end-to-end, including Phase 2 hand-off for `sentinel-sync` tuple writes.
- [[projects/rustycog/references/rustycog-http]]
- [[projects/rustycog/references/openfga-real-testcontainer-fixture]] — real OpenFGA fixture for service integration tests.
- [[projects/rustycog/references/openfga-mock-service]] — historical wiremock-backed `Check` fake; superseded for Hive/Manifesto/Telegraph integration tests by the real fixture.

## Removed

- `PermissionEngine`, `CasbinPermissionEngine`, `PermissionsFetcher` — replaced by `PermissionChecker` + OpenFGA. See [[entities/permissions-fetcher]] (marked removed).
