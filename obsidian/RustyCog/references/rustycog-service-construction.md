---
title: >-
  RustyCog Service Construction Guides
category: references
tags: [reference, rustycog, architecture, visibility/internal]
sources:
  - Manifesto/docs/rustycog-service-build-guide.md
  - Manifesto/docs/rustycog-hexagonal-web-service-guide.md
  - Manifesto/docs/rustycog-implementation-and-usage-guide.md
  - Manifesto/src/main.rs
  - Manifesto/configuration/src/lib.rs
  - Manifesto/setup/src/app.rs
  - Manifesto/http/src/lib.rs
  - Manifesto/application/src/command/factory.rs
  - rustycog/rustycog-command/src/registry.rs
  - rustycog/rustycog-config/src/lib.rs
  - rustycog/rustycog-http/src/builder.rs
  - rustycog/rustycog-permission/src/lib.rs
  - rustycog/rustycog-logger/src/lib.rs
summary: >-
  Manifesto-authored RustyCog build guides checked against current loader, routing, logging, command, and runtime behavior, preserving guide-vs-code drift as explicit conflicts.
provenance:
  extracted: 0.73
  inferred: 0.07
  ambiguous: 0.20
created: 2026-04-14T17:03:47.5107188Z
updated: 2026-04-19T11:49:06.1450368Z
---

# RustyCog Service Construction Guides

These guides use `[[projects/manifesto/manifesto]]` as a reference implementation for building services on top of `[[projects/rustycog/rustycog]]`. This page focuses on construction workflow and guide-vs-runtime drift, not crate API inventory.

## Key Ideas

- The practical assembly order remains: typed config, logging init, DB pool, repositories/services, command registry, app state, route builder, then integration tests.
- Composition root ownership stays explicit in setup crates so domain modules remain transport-agnostic.
- Permission-wired routes still follow the same sequence: model files + fetcher wiring + required permission checks.
- The guides remain useful as a procedural blueprint, but crate-level behavior now belongs to `[[projects/rustycog/references/index]]` and skills belong to `[[skills/building-rustycog-services]]`.
- Guide/runtime drift is material and must be tracked explicitly:
  - Some documented knobs are inert in current runtime paths (`service.component_service.timeout_seconds`, `[command.retry]` in Manifesto). ^[ambiguous]
  - Logging/bootstrap defaults in docs and live startup code still diverge (`setup_logging` guidance vs direct tracing init). ^[ambiguous]
  - README-era ergonomics around macros/examples are not visible in-tree. ^[ambiguous]
  - Packaging scope drift persists (`rustycog-server` health-only, `rustycog-logger` out of root workspace members). ^[ambiguous]

## Open Questions

- The guides are Manifesto-centric; cross-service conformance is still not mapped precisely.
- Which behavior is normative for new services when docs and runtime disagree: guide recommendation or live implementation? ^[ambiguous]
- Macro/example ergonomics promised by README-era docs are still unresolved in the checked-in tree. ^[ambiguous]

## Sources

- [[skills/building-rustycog-services]] — Procedural workflow distilled from these sources
- [[projects/rustycog/references/index]] — Crate-level behavior referenced by each construction step
- [[projects/rustycog/references/index]] — Compact crate inventory for this workflow
- [[concepts/shared-rust-microservice-sdk]] — Cross-project SDK framing
- [[projects/iamrusty/concepts/hexagonal-architecture]] — Service-boundary pattern the guides enforce