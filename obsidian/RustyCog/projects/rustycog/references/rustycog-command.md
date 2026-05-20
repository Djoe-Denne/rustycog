---
title: RustyCog Command
category: references
tags: [reference, rustycog, commands, visibility/internal]
sources:
  - rustycog/rustycog-command/src/lib.rs
  - rustycog/rustycog-command/src/generic_service.rs
  - rustycog/rustycog-command/src/registry.rs
  - rustycog/rustycog-command/src/token.rs
  - rustycog/rustycog-config/src/lib.rs
summary: rustycog::command provides the typed command runtime with CommandError boundaries, type-erased handler registration, retry/timeout orchestration, and registry-backed execution surfaces.
provenance:
  extracted: 0.9
  inferred: 0.06
  ambiguous: 0.04
created: 2026-04-15T17:15:56.0808743Z
updated: 2026-05-20T14:02:00Z
---

# RustyCog Command

`rustycog::command` (historically `rustycog-command`) implements the typed command runtime used by platform services and integrated into `[[projects/rustycog/references/rustycog-http]]`.

## Key Ideas

- `Command`, `CommandHandler`, and `CommandContext` define a common contract for validated command execution.
- The command layer uses its own `CommandError` categories (validation, authentication, business, infrastructure, timeout, retry exhausted) rather than directly exposing `ServiceError`.
- `CommandRegistry` stores type-erased handlers (`DynCommandHandler`) and orchestrates validation, timeout handling, retry logic, tracing, and metrics.
- `RetryPolicy` supports exponential backoff with optional jitter and classifies retryable errors (`Infrastructure` and `Timeout`).
- `CommandErrorMapper` lets services map domain/service-layer errors into command-layer errors at handler registration boundaries.
- `MetricsCollector` is pluggable; the default `LoggingMetricsCollector` records command duration, success/failure, retries, and error class.
- `RegistryConfig::from_retry_config()` bridges runtime retry settings from `rustycog-config`.
- `GenericCommandService` is the shared execution facade used by HTTP and other transport layers, and `ValidateTokenCommand` provides a built-in token-validation command shape.
- The command runtime remains transport-agnostic and can be reused by HTTP handlers, queue consumers, or test harnesses.

## Linked Entities

- [[entities/command-registry]]
- [[entities/command-context]]
- [[entities/service-error]]

## Open Questions

- The crate exposes mapper interfaces, but consistency of domain-to-command error mapping still depends on each service factory implementation. ^[inferred]

## Sources

- [[projects/rustycog/references/index]]
- [[concepts/command-registry-and-retry-policies]]
- [[projects/rustycog/rustycog]]
