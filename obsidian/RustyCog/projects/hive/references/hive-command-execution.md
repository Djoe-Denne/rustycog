---
title: Hive Command Execution
category: references
tags: [reference, commands, events, visibility/internal]
sources:
  - Hive/application/src/command/factory.rs
  - Hive/application/src/command/role.rs
  - Hive/http/src/handlers/roles.rs
  - Hive/setup/src/app.rs
  - Hive/application/src/usecase/organization.rs
  - Hive/application/src/usecase/invitation.rs
  - Hive/application/src/usecase/external_link.rs
  - Hive/application/src/usecase/sync_job.rs
summary: Hive routes organization-management behavior through a RustyCog command registry, but route wiring and registry coverage currently drift in a few role-related paths.
provenance:
  extracted: 0.79
  inferred: 0.12
  ambiguous: 0.09
created: 2026-04-14T18:56:22.3888182Z
updated: 2026-04-19T11:13:11Z
---

# Hive Command Execution

These sources explain how `[[projects/hive/hive]]` routes organization-management behavior through one typed command runtime, then turns successful domain operations into `[[projects/hive-events/hive-events]]` events for the rest of the platform.

## Key Ideas

- `HiveCommandRegistryFactory::create_hive_registry()` registers commands for organization create/get/update/delete/list/search, member add/remove/list/get/update, invitation create/list/cancel/accept/get-by-token/resend, external-link creation, and sync-job start.
- `setup/src/app.rs` wraps that registry in `GenericCommandService` and places it into `AppState`, making the command service the main bridge between HTTP handlers and use cases.
- Hive builds on the shared `[[projects/rustycog/references/rustycog-command]]` runtime rather than maintaining a service-local command bus implementation.
- Unlike `<!-- [[projects/telegraph/telegraph]] -->`, Hive does not run a queue consumer through the same command service; unlike `<!-- [[projects/iamrusty/iamrusty]] -->`, Hive does not visibly bind service-specific retry config into the registry even though `AppConfig` includes a `command` section. Conflict to resolve. ^[ambiguous]
- Registry and route coverage currently drift in both directions: some invitation/member commands exist without live routes, while role routes call `ListRolesCommand` and `GetRoleCommand` that are defined in code but not registered in `create_hive_registry()`. Conflict to resolve. ^[ambiguous]
- Hive's use cases are event-producing as well as state-changing: organizations publish created/updated/deleted events, invitations publish `InvitationCreatedEvent`, external links publish `ExternalLinkCreatedEvent`, and sync jobs publish `SyncJobStartedEvent`.
- This makes Hive a good example of a RustyCog service where command execution sits between HTTP and event emission rather than only between HTTP and a database write.

## Open Questions

- Because the command registry is broader than the router, readers need care not to assume every registered command is externally reachable over HTTP today. ^[ambiguous]
- Hive's retry config exists in TOML, but the current composition root does not obviously consume it when building the registry. Conflict to resolve. ^[ambiguous]

## Sources

- [[projects/hive/hive]] - Service whose handlers rely on this runtime.
- [[projects/hive-events/hive-events]] - Event contract crate used by Hive's publishing use cases.
- [[concepts/command-registry-and-retry-policies]] - Cross-service view of the registry differences.
- [[projects/hive/references/hive-http-api-and-openapi-drift]] - Route-level consumers of the command layer.
- [[projects/rustycog/references/rustycog-command]] - Shared command runtime primitives used by Hive.