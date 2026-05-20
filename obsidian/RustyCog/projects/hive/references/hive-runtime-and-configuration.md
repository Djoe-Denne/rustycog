---
title: Hive Runtime and Configuration
category: references
tags: [reference, configuration, integrations, visibility/internal]
sources:
  - Hive/config/default.toml
  - Hive/config/development.toml
  - Hive/config/test.toml
  - Hive/tests/sqs_event_routing_tests.rs
  - Hive/configuration/src/lib.rs
  - Hive/setup/src/app.rs
  - Hive/infra/src/event/event_adapter.rs
summary: Hive uses HIVE-prefixed typed config for DB, IAM, external-provider, command, and SQS destination-list queue settings.
provenance:
  extracted: 0.77
  inferred: 0.13
  ambiguous: 0.10
created: 2026-04-14T18:56:22.3888182Z
updated: 2026-04-25T11:25:00Z
---

# Hive Runtime and Configuration

These sources describe how `[[projects/hive/hive]]` is configured and started: the `HIVE` env prefix, typed config loading, environment-specific TOML overrides, DB and queue behavior, and the outbound service settings Hive uses to talk to IAM and external-provider systems.

## Key Ideas

- `AppConfig` implements `rustycog_config::ConfigLoader` with the env prefix `HIVE` and includes `server`, `database`, `iam_service`, `external_provider_service`, `logging`, `scaleway`, `command`, and `queue` sections, matching the shared `[[projects/rustycog/references/rustycog-config]]` runtime model.
- Default and development config enable SQS-style queue publishing with destination-list queue settings, while test keeps SQS queue names configured but `enabled = false` by default. `Hive/tests/sqs_event_routing_tests.rs` turns it on with `HIVE_QUEUE__ENABLED=true` before starting LocalStack, using `port = 0`, `default_queues = ["test-hive-default-events"]`, and `[queue.queues]` entries for SentinelSync-bound Hive events.
- Hive declares command retry settings in config, but dev and test set `max_attempts = 0` while default config uses `3`, so the live retry posture depends heavily on environment.
- `setup/src/app.rs` creates a `MultiQueueEventPublisher` from `config.queue` and `HiveErrorMapper`, so queue publishing is part of the normal runtime assembly rather than an optional bolt-on through `[[projects/rustycog/references/rustycog-events]]`.
- `Hive/tests/sqs_event_routing_tests.rs` validates that mapped organization/member events land on `test-sentinel-sync-events` and not on the fallback queue, proving the per-event `QueueConfig` routing rather than only publisher serialization.
- Conflict to resolve: unlike `<!-- [[projects/telegraph/telegraph]] -->`, Hive does not add a second service-specific queue-routing schema on top of `QueueConfig`; unlike `<!-- [[projects/iamrusty/iamrusty]] -->`, it adds explicit outbound `iam_service` and `external_provider_service` blocks instead. Both are valid `rustycog-config` service shapes. ^[ambiguous]
- Conflict to resolve: both `iam_service` and `external_provider_service` default to `localhost:8080` in `config/default.toml`, which is an operator-facing ambiguity until a stronger environment story pins them to distinct services. ^[ambiguous]

## Open Questions

- The config crate comment says “IAM service configuration,” which is correct but easy to misread as imported service config instead of Hive's outbound dependency settings.
- Hive now has a dedicated SQS routing test path, but it still does not run a Hive queue consumer; downstream tuple effects remain SentinelSync's responsibility. ^[inferred]

## Sources

- [[projects/hive/hive]] - Project page for the service using this runtime shape.
- [[concepts/structured-service-configuration]] - Cross-service comparison of config loader patterns.
- [[projects/hive/references/hive-command-execution]] - Registry and event publisher wiring that depends on this config.
- [[projects/hive/concepts/external-provider-sync-jobs]] - Domain flows that consume the outbound service config.
- [[projects/rustycog/references/rustycog-config]] - Shared config primitives reused by Hive.
- [[projects/rustycog/references/rustycog-events]] - Queue publisher runtime used for outbound Hive events.