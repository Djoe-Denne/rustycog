---
title: Using RustyCog Config
category: skills
tags: [rustycog, configuration, skills, visibility/internal]
sources:
  - rustycog/rustycog-config/src/lib.rs
summary: Step-by-step usage of rustycog-config for typed service config loading, env prefixes, and queue/db/runtime sections.
provenance:
  extracted: 0.88
  inferred: 0.06
  ambiguous: 0.06
created: 2026-04-15T17:15:56.0808743Z
updated: 2026-04-15T17:15:56.0808743Z
---

# Using RustyCog Config

Use this guide when wiring typed config with `<!-- [[projects/rustycog/references/rustycog-config]] -->`.

## Workflow

- Define your service config struct and implement `ConfigLoader` with the correct env prefix.
- Load startup config with `load_config_with_cache()` or `load_config_fresh()` depending on cache needs.
- Keep shared sections (`server`, `database`, `logging`, `queue`, `command`) aligned with RustyCog structs.
- Use `QueueConfig` as the single selector for Kafka/SQS/disabled behavior in event setup.
- Reserve `load_config_part()` for targeted reads and remember its section-based env prefixes.

## Common Pitfalls

- Assuming `load_config_part("server")` respects your service prefix instead of `SERVER_*`.
- Expecting `config/default.toml` to always be merged automatically.
- Defining queue or retry knobs in TOML but not wiring the corresponding runtime path.

## Sources

- <!-- [[projects/rustycog/references/rustycog-config]] -->
- <!-- [[entities/queue-config]] -->
- <!-- [[concepts/structured-service-configuration]] -->
