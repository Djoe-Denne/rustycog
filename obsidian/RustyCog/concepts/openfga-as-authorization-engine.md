---
title: OpenFGA as Authorization Engine
category: concept
tags: [concept, authorization, openfga, zanzibar]
summary: >-
  OpenFGA is the single authorization engine for AIForAll. It runs as its own process with its own Postgres store, holds a Zanzibar-shaped relation graph for every protected resource, and answers Check/Expand calls over HTTP and gRPC. Client services are thin callers.
updated: 2026-04-20
---

# OpenFGA as Authorization Engine

OpenFGA (the Auth0/Okta open-source Zanzibar implementation) replaces every per-service Casbin enforcer. It owns one authorization model and one relation store for the whole platform.

## Deployment shape

- Separate process, not embedded. Runs as a service in the root [`docker-compose.yml`](../../../docker-compose.yml) in local dev and as a managed service in shared environments.
- Backed by a dedicated `openfga_dev` database on the shared `postgres` instance — the relation graph is not co-located with any domain database, but it does share a Postgres server in dev to keep the stack small.
- Exposes HTTP on host port `8090` (container `8080`), gRPC on `8091`, and the Playground UI on `3000`. Service-to-service inside the docker network uses `http://openfga:8080`.

## Why not embedded

OpenFGA has no first-class Rust SDK. Running it as a sidecar keeps the Rust surface tiny (a `reqwest`-based client in `OpenFgaPermissionChecker`) and lets every service share one consistent decision log.

## Data flow

1. [[projects/sentinel-sync/sentinel-sync]] consumes business events (Hive, Manifesto, IAMRusty) and issues `Write`/`Delete` tuple calls to OpenFGA. It is the only writer.
2. Client services issue `Check` calls through [[entities/permission-checker]] before entering any domain-layer handler. They are the only readers.
3. Failures on either side are retried with backoff; tuple writes are idempotent via a `processed_events` ledger kept by `sentinel-sync`.

## Why OpenFGA and not Casbin

- **Zanzibar relations compose.** `admin from organization` on `project` expresses org-wide role inheritance in one line. The old Casbin models had to re-import each service's policies into every other service that wanted to reason about them.
- **Single source of truth.** One place to audit decisions, one model to evolve, one cache invalidation story.
- **Engine-neutral contract.** The `PermissionChecker` trait in `rustycog-permission` hides OpenFGA behind a small interface so SpiceDB or a hand-written evaluator could be swapped in later without touching HTTP routes.

## Related

- [[concepts/centralized-authorization-service]]
- [[concepts/zanzibar-relation-tuples]]
- [[projects/sentinel-sync/references/openfga-model]]
