---
title: RustyCog HTTP
category: references
tags: [reference, rustycog, http, visibility/internal]
sources:
  - rustycog/rustycog-http/src/lib.rs
  - rustycog/rustycog-http/src/builder.rs
  - monolith/src/runtime.rs
  - rustycog/rustycog-http/src/error.rs
  - rustycog/rustycog-http/src/extractors.rs
  - rustycog/rustycog-http/src/middleware_permission.rs
  - rustycog/rustycog-http/src/tracing_middleware.rs
  - rustycog/rustycog-http/src/jwt_handler.rs
summary: rustycog-http provides the Axum service shell, including router creation, reusable serving, auth wiring, centralized permission guards, validation, and tracing/error helpers.
provenance:
  extracted: 0.91
  inferred: 0.06
  ambiguous: 0.03
created: 2026-04-15
updated: 2026-05-20T14:02:00Z
---

# RustyCog HTTP

`rustycog::http` (historically `rustycog-http`) is the service-shell module that wires routes, middleware, auth extraction, permissions, and health checks for RustyCog services.

## Key Ideas

- `AppState` carries three shared handles: the `GenericCommandService`, a `UserIdExtractor`, and an `Arc<dyn PermissionChecker>` built once in the composition root and reused for every request.
- `RouteBuilder` gives a fluent setup API for routes, auth mode (`authenticated` / `might_be_authenticated`), a single permission guard method, middleware, and `/health`.
- `RouteBuilder::into_router()` finalizes routes into an Axum router without binding a port, which allows service routers to be nested inside `[[projects/aiforall/references/modular-monolith-runtime]]`.
- `serve_router(router, ServerConfig)` chooses HTTPS (`axum_server` + rustls) when `ServerConfig.tls_enabled`, otherwise plain HTTP.
- `RouteBuilder::build()` remains the standalone-service compatibility API and delegates to `into_router()` plus `serve_router()`.
- `ValidatedJson<T>` enforces body validation through the `validator` crate.
- Tracing middleware standardizes `x-correlation-id` and `x-request-id` propagation.

## Permission middleware

Permission checks go through `with_permission_on(required: Permission, object_type: &'static str)`. The middleware:

1. Pulls the UUID from the current request's extensions (set by the auth middleware).
2. Extracts the **deepest** UUID segment from the request path — that is the resource being operated on.
3. Builds a `ResourceRef { object_type, object_id }` of the requested OpenFGA type.
4. Calls `AppState.permission_checker.check(subject, required, resource)`.

There is no longer any `permissions_dir`, `resource`, or `with_permission_fetcher` step. The builder enforces only:

1. Optional `authenticated()` / `might_be_authenticated()`.
2. Optional `with_permission_on(Permission, object_type)`.

`object_type` must match an OpenFGA type defined in [openfga/model.fga](../../../../openfga/model.fga) (`"organization"`, `"project"`, `"component"`, `"notification"`).

## Example

```rust
RouteBuilder::new(state)
    .post("/organizations", create_org_handler)
    .authenticated()
    .get("/organizations/{org_id}", read_org_handler)
    .authenticated()
    .with_permission_on(Permission::Read, "organization")
    .put("/organizations/{org_id}", update_org_handler)
    .authenticated()
    .with_permission_on(Permission::Write, "organization")
    .build(server_config)
    .await?;
```

## Linked Entities

- [[entities/route-builder]]
- [[entities/permission-checker]]
- [[entities/resource-ref]]
- [[entities/resource-id]]

## Related

- [[projects/rustycog/references/rustycog-permission]]
- [[projects/sentinel-sync/sentinel-sync]]
- [[projects/aiforall/references/modular-monolith-runtime]]
- [[concepts/openfga-as-authorization-engine]]

## Sources

- [[projects/rustycog/references/index]]
- [[projects/rustycog/rustycog]]
