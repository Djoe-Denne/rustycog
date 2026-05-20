---
title: ServiceError
category: entities
tags: [rustycog, errors, runtime, visibility/internal]
sources:
  - rustycog/rustycog-core/src/error.rs
summary: ServiceError is RustyCog's shared runtime error envelope with category, retryability, and HTTP status mapping helpers.
provenance:
  extracted: 0.9
  inferred: 0.04
  ambiguous: 0.06
created: 2026-04-15T17:15:56.0808743Z
updated: 2026-04-15T22:10:00Z
---

# ServiceError

`ServiceError` is the main cross-layer runtime error type defined in `[[projects/rustycog/references/rustycog-core]]`.

## Key Ideas

- `ServiceError` is the shared runtime error envelope consumed by command, HTTP, and adapter layers.
- Category helpers (`http_status_code()`, `is_retryable()`) keep transport mapping and retry behavior consistent.
- Constructor helpers reduce ad hoc error-shape drift in services.
- Domain-layer specifics should originate in `DomainError` and convert into `ServiceError` at boundaries.

## Sources

- [[projects/rustycog/references/rustycog-core]]
- [[entities/domain-error]]
- [[projects/rustycog/references/rustycog-command]]
