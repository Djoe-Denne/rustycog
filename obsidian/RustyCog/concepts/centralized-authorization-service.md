---
title: Centralized Authorization Service
category: concept
tags: [concept, authorization, architecture, sentinel-sync]
summary: >-
  AIForAll centralizes authorization in OpenFGA with a small sentinel-sync worker feeding it tuples. Client services stop owning permission models and fetchers; they only call Check before entering domain logic.
updated: 2026-04-20
---

# Centralized Authorization Service

For a long time, each service (Hive, Manifesto, Telegraph) shipped its own Casbin `.conf` file plus a `PermissionsFetcher` implementation that loaded per-user/per-resource policy rows into a per-request enforcer. That produced three independent decision surfaces that nonetheless had to agree about cross-service questions like "can an org admin edit any project under their org?".

The centralized authorization service pattern fixes that by splitting the problem into three roles:

1. **Engine** — OpenFGA, the only place that answers "is this allowed?". See [[concepts/openfga-as-authorization-engine]].
2. **Sync worker** — [[projects/sentinel-sync/sentinel-sync]], the only writer into OpenFGA. It translates domain events into relation tuples.
3. **Client** — every HTTP-facing service, which only calls `Check` through [[entities/permission-checker]] and otherwise stays out of the authorization business.

## Deprecated pattern

The replaced pattern is documented (and marked deprecated) at [[concepts/resource-scoped-permission-fetchers]]. It survives in the archive for historical context only.

## Trade-offs

- **Latency**: each authz decision is now an out-of-process call. Mitigated by a short-TTL cache in the checker and a JWT fast-path for decisions resolvable from the access token.
- **Operational coupling**: every service depends on OpenFGA being reachable. Mitigated by caching plus explicit health checks on startup.
- **Eventual consistency**: relation writes are event-driven, so there is a window after a domain mutation where OpenFGA has not yet seen the new tuple. Acceptable for most flows; critical flows can issue a synchronous tuple write from the originating service.

## Related

- [[projects/sentinel-sync/sentinel-sync]]
- [[concepts/zanzibar-relation-tuples]]
- [[entities/permission-checker]]
