---
title: CommandContext
category: entities
tags: [rustycog, commands, tracing, visibility/internal]
sources:
  - rustycog/rustycog-command/src/lib.rs
summary: CommandContext carries execution-scoped metadata such as execution ID, optional user ID, request ID, and key-value metadata.
provenance:
  extracted: 0.9
  inferred: 0.05
  ambiguous: 0.05
created: 2026-04-15T17:15:56.0808743Z
updated: 2026-04-15T22:10:00Z
---

# CommandContext

`CommandContext` is the command-execution context type used by `[[projects/rustycog/references/rustycog-command]]`.

## Key Ideas

- `CommandContext` carries execution-scoped metadata (`execution_id`, optional `user_id`, optional `request_id`) across transport boundaries.
- It is the canonical context envelope for command execution, regardless of whether commands come from HTTP or queue consumers.
- The metadata map allows service-specific extension without changing `Command` trait signatures.
- Construction/enrichment patterns are documented in `[[projects/rustycog/references/rustycog-command]]`; this page is the noun definition.

## Sources

- [[projects/rustycog/references/rustycog-command]]
- [[entities/command-registry]]
- [[concepts/command-registry-and-retry-policies]]
