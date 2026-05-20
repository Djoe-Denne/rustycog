---
title: Using RustyCog Command
category: skills
tags: [rustycog, commands, skills, visibility/internal]
sources:
  - rustycog/rustycog-command/src/lib.rs
  - rustycog/rustycog-command/src/registry.rs
  - rustycog/rustycog-config/src/lib.rs
summary: Procedure for defining commands, registering handlers, and running the RustyCog command registry with retry and timeout policies.
provenance:
  extracted: 0.89
  inferred: 0.05
  ambiguous: 0.06
created: 2026-04-15T17:15:56.0808743Z
updated: 2026-04-15T17:15:56.0808743Z
---

# Using RustyCog Command

Use this guide when composing `<!-- [[projects/rustycog/references/rustycog-command]] -->`.

## Workflow

- Define one command struct per operation and implement `Command` (`command_type`, `command_id`, `validate`).
- Implement `CommandHandler<YourCommand>` and keep business logic in use cases/services.
- Register handlers through `CommandRegistryBuilder` with stable command keys and error mappers.
- Build `RegistryConfig` from runtime retry config when service policy should be externally configurable.
- Expose one shared command service in `AppState` so HTTP and queue adapters reuse the same execution path.

## Common Pitfalls

- Letting command key strings drift from `command_type()` values.
- Forgetting that `max_attempts = 0` disables retries entirely.
- Registering handlers in multiple places and creating split command surfaces.

## Sources

- <!-- [[projects/rustycog/references/rustycog-command]] -->
- <!-- [[entities/command-registry]] -->
- <!-- [[concepts/command-registry-and-retry-policies]] -->
