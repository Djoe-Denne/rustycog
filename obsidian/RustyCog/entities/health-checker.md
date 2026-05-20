---
title: HealthChecker
category: entities
tags: [rustycog, server, health, visibility/internal]
sources:
  - rustycog/rustycog-server/src/health.rs
summary: HealthChecker is the RustyCog trait for asynchronous component health checks, with HealthStatus as the common result shape.
provenance:
  extracted: 0.91
  inferred: 0.04
  ambiguous: 0.05
created: 2026-04-15T17:15:56.0808743Z
updated: 2026-04-15T22:10:00Z
---

# HealthChecker

`HealthChecker` is the health-probe abstraction exported by `[[projects/rustycog/references/rustycog-server]]`.

## Key Ideas

- `HealthChecker` is the async probe contract used by RustyCog health endpoints.
- `HealthStatus` provides the common result shape (`Healthy` or `Unhealthy(message)`).
- `BasicHealthChecker` is the default always-healthy implementation.
- This entity remains intentionally narrow because `rustycog-server` currently focuses on health primitives only.

## Sources

- [[projects/rustycog/references/rustycog-server]]
- [[entities/route-builder]]
- [[concepts/shared-rust-microservice-sdk]]
