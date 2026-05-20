---
title: RustyCog Core
category: references
tags: [reference, rustycog, sdk, visibility/internal]
sources:
  - rustycog/rustycog-core/src/error.rs
summary: rustycog-core defines the shared ServiceError and DomainError contracts that other RustyCog crates and services build on.
provenance:
  extracted: 0.88
  inferred: 0.05
  ambiguous: 0.07
created: 2026-04-15T17:15:56.0808743Z
updated: 2026-05-20T14:04:00Z
---

# RustyCog Core

`rustycog::core` (historically `rustycog-core`) is the base error vocabulary used across `[[projects/rustycog/rustycog]]` modules and consuming services.

## Key Ideas

- `ServiceError` encodes transport-ready categories such as validation, auth, business, infrastructure, not-found, conflict, timeout, and internal errors.
- Helper constructors (`validation_field`, `not_found_resource`, `infrastructure_with_source`, and others) keep error creation consistent across handlers and services.
- `ServiceError::category()`, `is_retryable()`, and `http_status_code()` centralize behavior that other crates reuse for retries and HTTP responses.
- `DomainError` is the domain-facing error enum and converts into `ServiceError` through `From<DomainError>`.
- The conversion layer makes `rustycog-command`, `rustycog-http`, and service code interoperable without each service inventing its own cross-layer error adapter.

## Linked Entities

- [[entities/service-error]]
- [[entities/domain-error]]

## Open Questions

- The `DomainError` docs still mention a specific service context while the crate is reused as a shared SDK contract. Conflict to resolve. ^[ambiguous]

## Sources

- [[projects/rustycog/references/index]]
- [[projects/rustycog/rustycog]]
- [[concepts/shared-rust-microservice-sdk]]
