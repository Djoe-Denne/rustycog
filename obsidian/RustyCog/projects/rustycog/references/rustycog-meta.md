---
title: RustyCog Meta (Legacy)
category: references
tags: [reference, rustycog, packaging, legacy, visibility/internal]
sources:
  - rustycog/Cargo.toml
  - rustycog/src/lib.rs
  - Cargo.toml
summary: rustycog-meta is a legacy packaging note; runtime consumers now use the unified rustycog crate with explicit feature selection.
provenance:
  extracted: 0.74
  inferred: 0.21
  ambiguous: 0.05
created: 2026-04-15T17:15:56.0808743Z
updated: 2026-05-20T14:00:00Z
---

# RustyCog Meta (Legacy)

`rustycog-meta` is kept as a legacy note for historical links. The active packaging model is now a unified `rustycog` crate with features, plus a separate `rustycog-testing` crate.

## Current Status

- `rustycog-meta` is not the recommended dependency target in this repository anymore.
- Runtime crates should consume `rustycog` and select only needed features (`core`, `config`, `http`, `events`, etc., or `full`).
- Integration tests should depend on `rustycog-testing`, which itself depends on `rustycog` (`full` + `test-utils`).
- Historical `rustycog-*` per-crate dependency guidance is deprecated.

## Migration Guidance

- Replace any historical "meta-package" advice with explicit `rustycog` feature selection.
- Keep module docs under the existing reference pages (`rustycog-command`, `rustycog-http`, etc.): these now describe feature-gated modules within `rustycog`.
- Use `[[projects/rustycog/references/index]]` as the canonical map.

## Example Dependency Shape

```toml
[dependencies]
rustycog = { path = "../rustycog", features = ["full"] }

[dev-dependencies]
rustycog-testing = { path = "../rustycog/rustycog-testing" }
```

## Linked Entities

- [[entities/command-registry]]
- [[entities/route-builder]]
- [[entities/domain-event]]

## Sources

- [[projects/rustycog/references/index]]
- [[projects/rustycog/rustycog]]
- [[concepts/shared-rust-microservice-sdk]]
