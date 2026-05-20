---
title: CommandRegistry
category: entities
tags: [rustycog, commands, orchestration, visibility/internal]
sources:
  - rustycog/rustycog-command/src/registry.rs
  - rustycog/rustycog-command/src/lib.rs
summary: CommandRegistry is the RustyCog execution hub that validates commands, routes handlers, enforces timeout/retry policy, and emits metrics.
provenance:
  extracted: 0.9
  inferred: 0.04
  ambiguous: 0.06
created: 2026-04-15T17:15:56.0808743Z
updated: 2026-04-15T22:10:00Z
---

# CommandRegistry

`CommandRegistry` is the typed command dispatch core from `[[projects/rustycog/references/rustycog-command]]`.

## Key Ideas

- `CommandRegistry` is the orchestration boundary between transports (HTTP/queue) and command handlers.
- The registry owns runtime policy application (validation, timeout, retry, metrics/tracing) for each execution.
- Setup code typically wires it with `CommandRegistryBuilder`, while retry parameters can be fed from `RegistryConfig`.
- Detailed runtime semantics belong to `[[projects/rustycog/references/rustycog-command]]`; this page stays glossary-level.

## Sources

- [[projects/rustycog/references/rustycog-command]]
- [[entities/command-context]]
- [[concepts/command-registry-and-retry-policies]]
