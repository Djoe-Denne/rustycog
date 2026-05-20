---
title: ResourceId
category: entities
tags: [rustycog, permissions, identifiers, visibility/internal]
sources:
  - rustycog/rustycog-permission/src/lib.rs
  - rustycog/rustycog-http/src/middleware_permission.rs
summary: ResourceId is the typed UUID wrapper RustyCog uses to pass route-scoped resources into permission fetchers and engines.
provenance:
  extracted: 0.9
  inferred: 0.04
  ambiguous: 0.06
created: 2026-04-15T17:15:56.0808743Z
updated: 2026-04-15T22:10:00Z
---

# ResourceId

`ResourceId` is a thin UUID wrapper that standardizes resource identity for permission checks.

## Key Ideas

- `ResourceId` is the typed route-resource identifier passed into permission resolution.
- Middleware extracts UUID-shaped path segments and emits ordered `ResourceId` values for authorization checks.
- Fetchers interpret these IDs using service-specific domain semantics.
- The wrapper avoids stringly-typed resource handling across permission middleware and engine layers.

## Sources

- [[projects/rustycog/references/rustycog-permission]]
- [[projects/rustycog/references/rustycog-http]]
- [[entities/permissions-fetcher]]
