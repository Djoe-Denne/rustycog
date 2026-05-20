---
title: RustyCog Config
category: references
tags: [reference, rustycog, configuration, visibility/internal]
sources:
  - rustycog/rustycog-config/src/lib.rs
summary: >-
  rustycog::config provides shared typed config primitives, including DB, OpenFGA, and QueueConfig with per-event SQS destination lists.
provenance:
  extracted: 0.89
  inferred: 0.07
  ambiguous: 0.04
created: 2026-04-15T17:15:56.0808743Z
updated: 2026-05-20T14:02:00Z
---

# RustyCog Config

`rustycog::config` (historically `rustycog-config`) is the typed configuration foundation for `[[projects/rustycog/rustycog]]` services.

## Key Ideas

- The crate defines shared runtime structs (`ServerConfig`, `DatabaseConfig`, `LoggingConfig`, `CommandConfig`, `KafkaConfig`, `SqsConfig`, `QueueConfig`, and `OpenFgaClientConfig`).
- `ConfigLoader` and `ConfigCache` traits let each service keep its own `AppConfig` while reusing one loading/caching mechanism.
- Capability traits (`HasDbConfig`, `HasQueueConfig`, `HasServerConfig`, `HasLoggingConfig`, `HasScalewayConfig`, `HasOpenFgaConfig`) make shared harness code depend on the sections it needs rather than on a concrete service config.
- `load_config_fresh()` and `load_config_with_cache()` choose config files from `RUN_ENV` and apply env overrides via service-specific prefixes.
- `load_config_part("server")` and similar helpers load one section at a time, but they use section-based env prefixes (`SERVER_*`, `QUEUE_*`, and so on).
- Queue support is transport-polymorphic through `QueueConfig::{Kafka,Sqs,Disabled}` so event code can switch transports without changing high-level calling code.
- `SqsConfig` now models fanout directly: `queues` maps each event type to a list of destination queue names, and `default_queues` provides fallback destinations for unmapped events.
- `SqsConfig` exposes routing helpers for `get_queue_names(event_type)`, `get_queue_urls(event_type)`, `all_queue_names()`, and `queue_url(queue_name)`, so publishers, consumers, and test fixtures share one queue-resolution contract.
- Random-port caching in DB/Kafka/SQS/OpenFGA config makes test runs stable once a random port is resolved for the process.
- `OpenFgaClientConfig` follows the DB/SQS host-plus-port shape: `scheme`, `host`, `port`, `store_id`, optional `authorization_model_id`, optional `api_token`, and optional `cache_ttl_seconds`. `api_url()` reconstructs the HTTP base URL and `actual_port()` resolves `port = 0` into a cached random port for testcontainers.

## Linked Entities

- [[entities/queue-config]]
- [[entities/db-connection-pool]]
- [[projects/rustycog/references/openfga-real-testcontainer-fixture]]

## Open Questions

- The SQS URL builder currently uses a Scaleway-style host format while other settings and naming remain AWS-oriented. Conflict to resolve. ^[ambiguous]
- The loader path still does not auto-merge a universal `config/default.toml` baseline before environment-specific files. ^[ambiguous]

## Sources

- [[projects/rustycog/references/index]]
- [[concepts/structured-service-configuration]]
- [[projects/rustycog/rustycog]]
