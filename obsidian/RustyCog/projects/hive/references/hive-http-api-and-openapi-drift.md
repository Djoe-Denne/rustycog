---
title: Hive HTTP API and OpenAPI Drift
category: references
tags: [reference, api, organizations, visibility/internal]
sources:
  - Hive/openspecs.yaml
  - Hive/http/src/lib.rs
  - Hive/http/src/handlers/roles.rs
  - Hive/http/src/error.rs
  - Hive/application/src/command/factory.rs
  - Hive/application/src/command/role.rs
  - Hive/application/src/dto/common.rs
  - Hive/tests/organization_api_tests.rs
  - Hive/tests/members_api_tests.rs
  - Hive/tests/external_link_api_tests.rs
summary: Source-backed comparison of Hive's live route table, command wiring, and richer custom HTTP error model against a larger OpenAPI contract that is not fully wired today.
provenance:
  extracted: 0.72
  inferred: 0.14
  ambiguous: 0.14
created: 2026-04-14T18:56:22.3888182Z
updated: 2026-04-19T11:13:11Z
---

# Hive HTTP API and OpenAPI Drift

These sources describe the synchronous API surface of `[[projects/hive/hive]]`, but they do not all tell the same story. The live route table, the handler set, the API tests, and `openspecs.yaml` each reveal a different slice of the shipped-versus-intended API.

## Key Ideas

- `http/src/lib.rs` is the authoritative live route table: public-or-optional-auth organization search/get, authenticated org create/update/delete/list, roles list/get, members add/remove/list/get, invitation creation, external-link creation, and sync-job start.
- `openspecs.yaml` documents a much larger API, including cursor-based list pagination, role CRUD, public invitation token routes, external-link CRUD, and per-link sync endpoints that are not registered in the live route builder. Conflict to resolve. ^[ambiguous]
- The handlers and command registry are broader than the route table: for example, invitation list/get/accept handlers and `update_member` exist in code, but are not currently wired into `create_app_routes()`. Conflict to resolve. ^[ambiguous]
- Role route wiring now drifts in the opposite direction too: `/roles` endpoints are registered and handlers invoke `ListRolesCommand`/`GetRoleCommand`, but those commands are not currently registered in `HiveCommandRegistryFactory::create_hive_registry()`. Conflict to resolve. ^[ambiguous]
- The API tests show behavior that already diverges from the spec: create-organization returns `200` rather than the spec's `201`, delete returns `200` rather than `204`, and tests use `page` and `page_size` instead of the spec's `cursor` and `limit` framing.
- Hive uses a custom `HttpError` plus `ApiErrorResponse` with `error_type`, `timestamp`, `request_id`, `details`, and `validation_errors`, which is a richer and different HTTP error surface from both IAMRusty's uniform wrapper and Telegraph's simpler error mapping. Conflict to resolve. ^[ambiguous]
- Some authenticated list routes are permission guarded without a path resource ID, so an authenticated user can still receive `403` until membership context is established by the service's permission model.

## Open Questions

- `openspecs.yaml` looks partly contract-first and partly stale; the repo needs a single declared source of truth for shipped versus planned HTTP operations.
- The spec mentions rate limiting, but the current source set does not show concrete route-level rate-limit middleware or configuration. ^[ambiguous]

## Sources

- [[projects/hive/hive]] - Project page for the service exposing these routes.
- [[projects/hive/concepts/organization-resource-authorization]] - Permission model attached to the live routes.
- [[projects/hive/references/hive-command-execution]] - Registry breadth versus live route exposure.
- [[projects/hive/references/hive-testing-and-api-fixtures]] - Tests that reveal the actual HTTP contract.