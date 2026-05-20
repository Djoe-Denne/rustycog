---
title: Telegraph Runtime and Configuration
category: references
tags: [reference, configuration, events, visibility/internal]
sources:
  - README.md
  - Telegraph/config/default.toml
  - Telegraph/config/development.toml
  - Telegraph/config/test.toml
  - Telegraph/configuration/src/lib.rs
  - Telegraph/setup/src/app.rs
  - Telegraph/docker-compose.yml
summary: Telegraph-specific configuration notes layered on top of RustyCog's shared config model, especially its queue-routing, communication, and local delivery settings.
provenance:
  extracted: 0.75
  inferred: 0.13
  ambiguous: 0.12
created: 2026-04-14T18:18:24.0602572Z
updated: 2026-04-19T12:08:26.9393504Z
---

# Telegraph Runtime and Configuration

This page narrows `[[projects/rustycog/references/rustycog-config]]` to the settings and drifts that are specific to `[[projects/telegraph/telegraph]]`.

## RustyCog Baseline

- `[[projects/rustycog/references/rustycog-config]]` explains the shared typed loader, env-prefix behavior, and queue-config primitives this service reuses.
- `[[concepts/structured-service-configuration]]` captures the cross-service config pattern that Telegraph specializes.
- `[[projects/rustycog/references/rustycog-events]]` provides the transport-level queue model that the local `queue` section builds on.

## Service-Specific Differences

- `TelegraphConfig` implements the shared `rustycog_config::ConfigLoader` traits with the env prefix `TELEGRAPH`, not the `IAM` prefix used by `IAMRusty`.
- The config model separates transport-level queue access (`queue`) from Telegraph-specific event routing (`queues`), so one block defines how to reach SQS while another block maps concrete event names to `modes` and optional template names.
- Development config points at `localstack:4566` and real SMTP infrastructure, while test config uses random DB and SQS ports plus local SMTP on port `1025`.
- `TemplateConfig` is fully configurable and points at `resources/templates` in live TOML files, but `setup/src/app.rs` still hardcodes `resources/communication_descriptor` for descriptor loading instead of treating it as configuration.
- The default config advertises `[communication.sms]`, but the current `CommunicationConfig` struct only includes `email`, `notification`, and `template`. ^[ambiguous]
- The root README says Telegraph runs on port `8081` in the shared stack, while `Telegraph/docker-compose.yml` exposes `8080:8080` for the service's local compose workflow. ^[ambiguous]

## Open Questions

- The top-level README talks about the `user-events` queue, while Telegraph's per-queue routing examples live under `test-user-events`; the repo needs one clearer naming story for operators. ^[ambiguous]
- Descriptor paths remain hardcoded in setup even though template paths are already configurable, which suggests a partially completed configuration story. ^[inferred]

## Sources

- [[projects/telegraph/telegraph]] - Project page for the service that consumes this configuration.
- [[concepts/structured-service-configuration]] - Cross-service comparison of config loader patterns.
- [[projects/rustycog/references/rustycog-config]] - Crate-level loader and queue-config details Telegraph builds on.
- [[projects/telegraph/concepts/multi-channel-delivery-modes]] - Delivery-model implications of the communication config.
- [[projects/telegraph/references/telegraph-event-processing]] - Queue-routing behavior driven by the `queue` and `queues` blocks.
