---
title: RustyCog Logger
category: references
tags: [reference, rustycog, logging, visibility/internal]
sources:
  - rustycog/rustycog-logger/src/lib.rs
  - rustycog/rustycog-config/src/lib.rs
  - rustycog/Cargo.toml
summary: rustycog-logger centralizes tracing initialization, including feature-flagged Scaleway Loki wiring and safe repeated setup behavior for tests and nested startup paths.
provenance:
  extracted: 0.9
  inferred: 0.06
  ambiguous: 0.04
created: 2026-04-15T17:15:56.0808743Z
updated: 2026-05-20T14:01:00Z
---

# RustyCog Logger

`rustycog::logger` (historically `rustycog-logger`) standardizes tracing initialization for services that implement `HasLoggingConfig` (and `HasScalewayConfig` when Loki support is enabled).

## Key Ideas

- `setup_logging()` maps configured log level/filter into an `EnvFilter` chain and initializes tracing layers.
- Logging setup uses `try_init()` to avoid panics when tests or nested startup paths initialize tracing multiple times.
- Under the `scaleway-loki` feature, the crate can attach Loki output using config-driven endpoint and token settings.
- Loki labels are sourced from `JOB` and `SERVICE` environment variables (with `unknown` fallbacks), then sent with the configured cockpit token.
- The trait alias `ServiceLoggerConfig` changes by feature flag so services only need the capabilities required by the active build (`HasLoggingConfig` always; `HasScalewayConfig` when Loki is enabled).

## Linked Entities

- [[entities/queue-config]]
- [[entities/service-error]]

## Open Questions

- Loki configuration still mixes provider-specific naming conventions across services and deserves a dedicated normalization pass. ^[inferred]

## Sources

- [[projects/rustycog/references/index]]
- [[concepts/structured-service-configuration]]
- [[projects/rustycog/rustycog]]
