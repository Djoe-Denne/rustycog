---
title: DomainError
category: entities
tags: [rustycog, errors, domain, visibility/internal]
sources:
  - rustycog/rustycog-core/src/error.rs
summary: DomainError is the domain-layer error enum that RustyCog maps into ServiceError for transport and orchestration layers.
provenance:
  extracted: 0.89
  inferred: 0.05
  ambiguous: 0.06
created: 2026-04-15T17:15:56.0808743Z
updated: 2026-04-15T22:10:00Z
---

# DomainError

`DomainError` is the domain-facing error type in `[[projects/rustycog/references/rustycog-core]]`.

## Key Ideas

- `DomainError` is the domain-layer failure vocabulary, separate from transport/runtime concerns.
- It keeps use-case and domain services explicit while allowing conversion into `ServiceError` at boundaries.
- The `From<DomainError> for ServiceError` bridge is the standard handoff into command and HTTP layers.
- Detailed variant behavior belongs to `[[projects/rustycog/references/rustycog-core]]`.

## Sources

- [[projects/rustycog/references/rustycog-core]]
- [[entities/service-error]]
- [[projects/rustycog/references/rustycog-command]]
