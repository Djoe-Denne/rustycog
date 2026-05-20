---
title: Using RustyCog Logger
category: skills
tags: [rustycog, logging, skills, visibility/internal]
sources:
  - rustycog/rustycog-logger/src/lib.rs
  - rustycog/rustycog-config/src/lib.rs
summary: Procedure for initializing tracing with rustycog-logger and wiring config-driven level/filter/Loki behavior.
provenance:
  extracted: 0.88
  inferred: 0.05
  ambiguous: 0.07
created: 2026-04-15T17:15:56.0808743Z
updated: 2026-04-15T17:15:56.0808743Z
---

# Using RustyCog Logger

Use this guide when initializing tracing through `<!-- [[projects/rustycog/references/rustycog-logger]] -->`.

## Workflow

- Ensure your app config implements `HasLoggingConfig` (and `HasScalewayConfig` if Loki feature is enabled).
- Call `setup_logging(&config)` once early in startup before building long-lived components.
- Set `logging.level` for coarse control and `logging.filter` for targeted directive overrides.
- Enable Loki integration only when running with the matching feature and valid remote credentials.
- Keep tracing init in one place to avoid competing global subscriber setup.

## Common Pitfalls

- Calling logging setup repeatedly in test/runtime paths and expecting reinitialization semantics.
- Mixing manual `tracing_subscriber` setup with `setup_logging()` in the same process.
- Enabling Loki feature without complete config/credentials.

## Sources

- <!-- [[projects/rustycog/references/rustycog-logger]] -->
- <!-- [[projects/rustycog/references/rustycog-config]] -->
- <!-- [[concepts/structured-service-configuration]] -->
