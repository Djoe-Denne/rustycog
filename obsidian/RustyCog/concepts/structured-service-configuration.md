---
title: Structured Service Configuration
category: concepts
tags: [configuration, env, rust, visibility/internal]
sources:
  - IAMRusty/docs/DATABASE_CONFIGURATION.md
  - IAMRusty/configuration/src/lib.rs
  - IAMRusty/config/default.toml
  - IAMRusty/config/test.toml
  - Telegraph/configuration/src/lib.rs
  - Telegraph/config/default.toml
  - Telegraph/config/development.toml
  - Telegraph/config/test.toml
  - Hive/configuration/src/lib.rs
  - Hive/config/default.toml
  - Hive/config/development.toml
  - Hive/config/test.toml
  - Manifesto/configuration/src/lib.rs
  - Manifesto/config/default.toml
  - Manifesto/config/development.toml
  - Manifesto/config/test.toml
  - Manifesto/docs/rustycog-service-build-guide.md
  - Manifesto/docs/rustycog-implementation-and-usage-guide.md
  - Manifesto/src/main.rs
  - rustycog/rustycog-config/src/lib.rs
summary: AIForAll services use typed config loaders, but IAMRusty, Telegraph, Hive, and Manifesto diverge in env prefixes, loader behavior, queue models, and service-specific sections.
provenance:
  extracted: 0.65
  inferred: 0.10
  ambiguous: 0.25
created: 2026-04-14T17:46:37.6929647Z
updated: 2026-04-15T22:10:00Z
---

# Structured Service Configuration

Across `<!-- [[projects/iamrusty/iamrusty]] -->`, `<!-- [[projects/telegraph/telegraph]] -->`, `<!-- [[projects/hive/hive]] -->`, and `<!-- [[projects/manifesto/manifesto]] -->`, configuration is treated as typed runtime state rather than a loose collection of env vars. All four services build on shared loaders from `[[projects/rustycog/rustycog]]`, but they organize service-specific concerns differently.

## Key Ideas

- IAMRusty's `AppConfig` combines server, database, OAuth, JWT, logging, command, queue, and legacy Kafka sections into one loadable structure under the `IAM` env prefix.
- Telegraph's `TelegraphConfig` uses the `TELEGRAPH` env prefix and layers service-specific `queues` and `communication` blocks on top of shared `ServerConfig`, `QueueConfig`, `DatabaseConfig`, and logging traits.
- Hive's `AppConfig` uses the `HIVE` env prefix and keeps the shared `QueueConfig` shape, but adds outbound `iam_service`, `external_provider_service`, and `command` sections for an HTTP-first service that publishes organization events.
- Manifesto's `ManifestoConfig` uses the `MANIFESTO` env prefix and combines server, logging, queue, database, scaleway, and `service.component_service` sections, but the current runtime only consumes some of those fields end to end.
- Across these services, environment-specific TOML plus `RUN_ENV` loading keeps runtime shape typed while allowing test-specific overrides (`port = 0`, queue toggles, etc.).
- Crate-level loader/struct mechanics are documented in `[[projects/rustycog/references/rustycog-config]]`; this page tracks service-level divergence.
- The RustyCog loader selects one primary file by `RUN_ENV`, and `load_config_part("server")` uses `SERVER_*`-style overrides instead of the service prefix. Conflict to resolve. ^[ambiguous]
- Telegraph separates queue transport (`queue`) from event routing (`queues.*.events`, per-event `modes`, optional `template` names), which gives it a more communication-pipeline-specific config shape than IAMRusty's single `AppConfig` pattern.
- Hive's config also includes `command.retry`, but unlike IAMRusty's current documented runtime the live composition path does not obviously bind that retry config into the registry. Conflict to resolve. ^[ambiguous]
- Conflict to resolve: IAMRusty consolidates queue/runtime policy into one service config model, Telegraph adds a second queue-routing schema and channel-specific `communication.*` sections, and Hive keeps one queue block but adds explicit outbound service sections. All three `rustycog-config` service shapes coexist today. ^[ambiguous]
- Conflict to resolve: Manifesto's docs present `config/default.toml` as a base layer, but its current loader path does not automatically merge that file and still leaves `logging.level`, `[command.retry]`, and `service.component_service.timeout_seconds` only partly wired. ^[ambiguous]
- Telegraph's `config/default.toml` documents `[communication.sms]`, but `CommunicationConfig` currently includes `email`, `notification`, and `template` only. Conflict to resolve. ^[ambiguous]
- Conflict to resolve: `SqsConfig` naming and URL construction mix AWS credential vocabulary with a Scaleway-style endpoint pattern (`https://sqs.<region>.scaleway.com/...`), so operator intent is not fully explicit. ^[ambiguous]

## Open Questions

- Root docs and service-local docs still mix multiple operator-facing stories: IAMRusty's docs drift between `APP_` and `IAM_`, and Telegraph's top-level README port story does not match its local compose file. ^[ambiguous]
- Hive's default config points both `iam_service` and `external_provider_service` at `localhost:8080`, which is operationally ambiguous until environment conventions make those dependencies distinct. ^[ambiguous]
- Telegraph already makes `template_dir` configurable, but descriptor loading is still hardcoded in setup rather than being part of the config model. ^[inferred]

## Sources

- <!-- [[projects/iamrusty/iamrusty]] --> - Service using the `IAM`-prefixed `AppConfig` variant.
- <!-- [[projects/telegraph/telegraph]] --> - Service using `TELEGRAPH` plus queue-routing and communication sections.
- <!-- [[projects/hive/hive]] --> - Service using `HIVE` plus outbound IAM and external-provider sections.
- <!-- [[projects/iamrusty/references/iamrusty-runtime-and-security]] --> - IAMRusty-specific runtime, JWT, and queue details.
- <!-- [[projects/telegraph/references/telegraph-runtime-and-configuration]] --> - Telegraph-specific queue, template, SMTP, and port behavior.
- <!-- [[projects/hive/references/hive-runtime-and-configuration]] --> - Hive-specific command, queue, and outbound service behavior.
- <!-- [[projects/manifesto/manifesto]] --> - Manifesto's concrete `MANIFESTO_*` loader path and partially wired config sections.
- [[projects/rustycog/rustycog]] - Shared SDK project that provides config primitives across services.
- [[projects/rustycog/references/rustycog-config]] - Crate-level details for typed config and queue structs.
- [[entities/queue-config]] - Shared queue transport selector entity.
- [[concepts/integration-testing-with-real-infrastructure]] - Real-infrastructure tests rely on these config shapes.