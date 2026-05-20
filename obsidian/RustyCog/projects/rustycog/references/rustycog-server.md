---
title: RustyCog Server
category: references
tags: [reference, rustycog, server, visibility/internal]
sources:
  - rustycog/rustycog-server/src/health.rs
summary: rustycog-server currently exposes lightweight health-check abstractions rather than a full application bootstrap layer.
provenance:
  extracted: 0.9
  inferred: 0.04
  ambiguous: 0.06
created: 2026-04-15T17:15:56.0808743Z
updated: 2026-05-20T14:01:00Z
---

# RustyCog Server

`rustycog::server` (historically `rustycog-server`) is the minimal health-check module in the unified RustyCog crate.

## Key Ideas

- `HealthStatus` encodes `Healthy` or `Unhealthy(message)` responses.
- `HealthChecker` defines one async `check()` contract for pluggable health probes.
- `BasicHealthChecker` is the default implementation and always returns healthy.
- Services typically reach this surface via the `server` feature on `rustycog`.

## Usage Guidance

- This crate is intentionally narrow; most server bootstrapping belongs to `[[projects/rustycog/references/rustycog-http]]`.
- In practice, teams use `HealthChecker` as a shared probe contract and keep route/server assembly in HTTP/setup crates.

## Linked Entities

- [[entities/health-checker]]

## Open Questions

- The crate name suggests broader server-setup ownership, but the current surface is health-only. Conflict to resolve. ^[ambiguous]

## Sources

- [[projects/rustycog/references/index]]
- [[projects/rustycog/rustycog]]
- [[concepts/shared-rust-microservice-sdk]]
