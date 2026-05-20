---
title: Command Registry and Retry Policies
category: concepts
tags: [commands, reliability, rust, visibility/internal]
sources:
  - IAMRusty/docs/COMMAND_PATTERN.md
  - IAMRusty/docs/COMMAND_RETRY_CONFIGURATION.md
  - IAMRusty/application/src/command/factory.rs
  - IAMRusty/config/test.toml
  - Telegraph/application/src/command/factory.rs
  - Telegraph/setup/src/app.rs
  - Hive/application/src/command/factory.rs
  - Hive/setup/src/app.rs
  - Hive/config/default.toml
  - Manifesto/application/src/command/factory.rs
  - Manifesto/docs/rustycog-implementation-and-usage-guide.md
summary: Repo services use typed command registries to centralize handlers, but IAMRusty, Telegraph, Hive, and Manifesto diverge in retry wiring, registry breadth, and transport entrypoints.
provenance:
  extracted: 0.69
  inferred: 0.10
  ambiguous: 0.21
created: 2026-04-14T17:46:37.6929647Z
updated: 2026-04-15T22:10:00Z
---

# Command Registry and Retry Policies

RustyCog request handlers and queue consumers delegate into typed command registries instead of calling use cases directly. The shared `[[projects/rustycog/rustycog]]` command layer provides one orchestration surface that consuming services can configure differently.

## Key Ideas

- IAMRusty's `CommandRegistryFactory::create_iam_registry` registers the service's auth-heavy command set and binds retry behavior from `CommandConfig`, so retry policy is part of the live runtime assembly.
- Telegraph's `TelegraphCommandRegistryFactory::create_telegraph_registry` registers `process_event`, `get_notifications`, `get_unread_count`, and `mark_notification_read`, then injects the resulting `GenericCommandService` into both `AppState` and the SQS-backed event consumer.
- Hive's `HiveCommandRegistryFactory::create_hive_registry` registers organization, member, invitation, external-link, and sync-job commands, then injects the resulting `GenericCommandService` into `AppState` for an HTTP-first service that also publishes domain events.
- Manifesto's `ManifestoCommandRegistryFactory::create_manifesto_registry` registers project, component, and member commands through grouped handler sets, and its implementation guide makes string equality between `command_type()` and registration key an explicit runtime contract.
- Crate-level mechanics (validation/timeout/retry/metrics pipeline) are documented in `[[projects/rustycog/references/rustycog-command]]`; this page focuses on cross-service wiring choices.
- In both services, command types are paired with dedicated handlers and error mappers, which keeps domain and infrastructure failures from leaking raw details into transport code.
- The command layer remains the main bridge between transport and use cases: HTTP handlers and queue-driven event handlers can both delegate into the same registry-backed service.
- Conflict to resolve: IAMRusty explicitly configures registry retry behavior, while Telegraph, Hive, and Manifesto currently build registries with plain `CommandRegistryBuilder::new()` and no visible service-specific retry binding even when docs or TOML advertise command retry configuration. Both `rustycog` usage patterns exist in the live repo. ^[ambiguous]
- IAMRusty's current test config sets `max_attempts = 0`, which already makes its live test retry posture stricter than many of the surrounding docs imply. ^[ambiguous]

## Open Questions

- Should HTTP-first services like Hive and queue-first services like Telegraph standardize on the same explicit registry retry configuration that IAMRusty binds through `CommandConfig`? Conflict to resolve. ^[ambiguous]
- Queue-driven command execution currently uses a thinner `CommandContext` than authenticated HTTP requests, so cross-service context conventions are still evolving. ^[inferred]

## Sources

- [[projects/rustycog/references/rustycog-command]] - Runtime command module reference.
- [[entities/command-registry]] - Registry entity reference.
- [[projects/rustycog/rustycog]] - Shared SDK project that owns the generic command runtime primitives.
- [[projects/rustycog/references/rustycog-command]] - Crate-level command runtime details.
- [[entities/command-registry]] - Shared runtime execution entity.
- [[entities/command-context]] - Shared execution context entity.
- [[concepts/structured-service-configuration]] - Retry and registry wiring still depend on each service's config approach.