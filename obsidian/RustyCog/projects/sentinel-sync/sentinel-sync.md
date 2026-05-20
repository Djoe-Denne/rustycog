---
title: Sentinel Sync
category: project
tags: [project, authorization, openfga, sentinel-sync, events]
summary: >-
  Centralized authorization for AIForAll. OpenFGA holds the Zanzibar relation store; the sentinel-sync worker translates business events from Hive, Manifesto, IAMRusty, and Telegraph into relation tuples. Client services only call OpenFGA's Check API.
updated: 2026-04-20
---

# Sentinel Sync

The `sentinel-sync` project replaces per-service Casbin authorization with a single Zanzibar-shaped store backed by OpenFGA. Client services no longer own `.conf` files or `PermissionsFetcher` implementations — they only call `Check` through [[entities/permission-checker]].

## Pieces

- **OpenFGA server** — the only authorization engine. Deployed as its own process with its own Postgres store. Model lives at [openfga/model.fga](../../../openfga/model.fga).
- **sentinel-sync worker** — a Rust binary that consumes events from Hive, Manifesto, and IAMRusty, translates them into OpenFGA `Write`/`Delete` tuple calls, and records every processed `event_id` for idempotency.
- **rustycog-permission** — shrunk to a `PermissionChecker` trait plus an `OpenFgaPermissionChecker` client. All Casbin code removed.
- **rustycog-http** middleware — injects `Arc<dyn PermissionChecker>` through `AppState` and exposes `with_permission_on(permission, object_type)` as the only authz builder method.

## Knowledge areas

- References: [[projects/sentinel-sync/references/openfga-model]], [[projects/sentinel-sync/references/sentinel-sync-worker]], [[projects/sentinel-sync/references/event-to-tuple-mapping]]
- Skills: [[projects/sentinel-sync/skills/extending-sentinel-sync-with-new-events]]
- Concepts: [[concepts/centralized-authorization-service]], [[concepts/openfga-as-authorization-engine]], [[concepts/zanzibar-relation-tuples]]

## Why centralize

Per-service Casbin engines meant that cross-service decisions (e.g. "this user is an org admin in Hive, so they implicitly admin every Manifesto project under that org") required each service to pull state from the others synchronously. OpenFGA's relation graph lets us express those inheritances declaratively (`admin from organization` on `project`) and answer all decisions with one `Check` RPC.
