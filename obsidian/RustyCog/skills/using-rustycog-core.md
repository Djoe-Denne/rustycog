---
title: Using RustyCog Core
category: skills
tags: [rustycog, errors, skills, visibility/internal]
sources:
  - rustycog/rustycog-core/src/error.rs
summary: Practical steps for adopting rustycog-core error primitives and keeping error behavior consistent across layers.
provenance:
  extracted: 0.89
  inferred: 0.05
  ambiguous: 0.06
created: 2026-04-15T17:15:56.0808743Z
updated: 2026-04-15T17:15:56.0808743Z
---

# Using RustyCog Core

Use this guide when adopting `<!-- [[projects/rustycog/references/rustycog-core]] -->` in a service.

## Workflow

- Define domain-layer failures with `DomainError` constructors so use-case code stays explicit.
- Convert domain errors at application boundaries into `ServiceError` (directly or via `From<DomainError>`).
- Use `ServiceError` constructors consistently in handlers and adapters instead of handwritten status/message pairs.
- Rely on `http_status_code()` and `is_retryable()` semantics in upper layers rather than duplicating category logic.

## Common Pitfalls

- Mixing custom ad hoc error enums with `ServiceError` in the same execution path.
- Dropping field/resource context when converting domain errors.
- Treating all failures as retryable instead of honoring `ServiceError` category semantics.

## Sources

- <!-- [[projects/rustycog/references/rustycog-core]] -->
- <!-- [[entities/service-error]] -->
- <!-- [[entities/domain-error]] -->
