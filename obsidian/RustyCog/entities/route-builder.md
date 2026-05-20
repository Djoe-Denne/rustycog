---
title: RouteBuilder
category: entities
tags: [rustycog, http, routing, visibility/internal]
sources:
  - rustycog/rustycog-http/src/builder.rs
  - rustycog/rustycog-http/src/lib.rs
summary: RouteBuilder is RustyCog's fluent HTTP composition API for route wiring, auth modes, a centralized permission guard backed by an injected PermissionChecker, and shared middleware layering.
updated: 2026-04-20
---

# RouteBuilder

`RouteBuilder` is the service-shell builder used in [[projects/rustycog/references/rustycog-http]].

## Surface

- Route registration: `.get`, `.post`, `.put`, `.patch`, `.delete`, `.route`, `.nest`, `.health_check`.
- Auth mode: `.authenticated()` or `.might_be_authenticated()`.
- Permission guard: `.with_permission_on(Permission, object_type)`.
- Startup: `.build(ServerConfig)` — HTTPS when `tls_enabled`, otherwise plain HTTP.

## Permission wiring

There is a single authz knob: `.with_permission_on(permission, object_type)`. The `AppState` already carries an `Arc<dyn PermissionChecker>` built once in the composition root (typically a `CachedPermissionChecker` wrapping `OpenFgaPermissionChecker`). The middleware pulls the deepest UUID from the request path, builds a `ResourceRef`, and delegates to the shared checker.

The previous knobs (`permissions_dir`, `resource`, `with_permission_fetcher`) have been removed.

## Composition-root rule

Build `AppState` once with:

```rust
AppState::new(command_service, user_id_extractor, permission_checker)
```

and share it across the entire process. Never build a per-request checker.

## Sources

- [[projects/rustycog/references/rustycog-http]]
- [[entities/permission-checker]]
- [[entities/resource-ref]]
- [[entities/resource-id]]
