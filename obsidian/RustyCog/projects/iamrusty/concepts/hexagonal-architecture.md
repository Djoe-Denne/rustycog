---
title: Hexagonal Architecture
category: concepts
tags: [architecture, hexagonal, ddd, visibility/internal]
sources:
  - IAMRusty/docs/ARCHITECTURE.md
  - IAMRusty/setup/src/app.rs
  - IAMRusty/http/src/lib.rs
summary: IAMRusty applies a generic RustyCog-style service split but adds IAM-specific OAuth, token, and setup boundaries that matter more than the generic layer names alone.
provenance:
  extracted: 0.79
  inferred: 0.16
  ambiguous: 0.05
created: 2026-04-14T17:46:37.6929647Z
updated: 2026-04-19T12:08:26.9393504Z
---

# Hexagonal Architecture

`[[projects/iamrusty/iamrusty]]` uses a hexagonal layout where domain services and ports stay isolated from HTTP, database, provider, and queue adapters. This page focuses on how IAMRusty applies that pattern differently from the generic RustyCog service story.

## RustyCog Baseline

- `[[references/rustycog-service-construction]]` and `[[skills/building-rustycog-services]]` describe the generic service layering and composition-root pattern that IAMRusty inherits.
- `[[projects/rustycog/references/rustycog-command]]`, `[[projects/rustycog/references/rustycog-http]]`, `[[projects/rustycog/references/rustycog-config]]`, `[[projects/rustycog/references/rustycog-db]]`, and `[[projects/rustycog/references/rustycog-events]]` explain the shared framework roles mapped across IAMRusty's layers.

## Service-Specific Differences

- The domain crate owns entities, ports, and business services such as OAuth, provider linking, token issuance, and email/password auth.
- The application crate wraps those services in use cases and typed commands, while `http/src/lib.rs` exposes the resulting behavior through validated routes and a shared `RouteBuilder`.
- The infrastructure crate implements repositories, OAuth clients, JWT encoders, password adapters, and event adapters, keeping external concerns out of the domain layer.
- `setup/src/app.rs` is the composition root, but IAMRusty treats setup and configuration as especially important first-class crates because security policy, provider wiring, and token services are central to runtime behavior.
- The runtime uses separate OAuth and token-repository instances for login, provider linking, and internal provider-token retrieval, which keeps flows isolated even when they share the same domain abstractions.

## Open Questions

- The architecture guide mostly describes four layers, but the current repo also treats configuration and setup as first-class crates with their own runtime responsibilities. ^[ambiguous]
- Some doc examples still show older route names and DTO shapes, so not every example in the architecture docs matches the live HTTP surface exactly. ^[ambiguous]

## Sources

- [[projects/iamrusty/iamrusty]] - Service where the pattern is applied concretely.
- [[projects/iamrusty/references/iamrusty-service]] - Crate map and composition-root wiring.
- [[concepts/structured-service-configuration]] - Runtime config layer that supports the architecture in practice.
- [[concepts/command-registry-and-retry-policies]] - Cross-cutting orchestration built on top of the layered design.
