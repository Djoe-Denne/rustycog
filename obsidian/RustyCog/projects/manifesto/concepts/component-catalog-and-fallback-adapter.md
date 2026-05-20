---
title: Component Catalog and Fail-Closed Adapter
category: concepts
tags: [components, integrations, projects, visibility/internal]
sources:
  - Manifesto/README.md
  - Manifesto/infra/src/adapters/component_service_client.rs
  - Manifesto/application/src/usecase/component.rs
  - Manifesto/configuration/src/lib.rs
  - Manifesto/setup/src/app.rs
  - Manifesto/tests/component_service_client_tests.rs
summary: >-
  Manifesto validates component types through an external component service and now fails closed
  on upstream errors instead of falling back to a mock catalog.
provenance:
  extracted: 0.88
  inferred: 0.08
  ambiguous: 0.04
created: 2026-04-14T20:25:00Z
updated: 2026-04-19T18:00:00Z
---

# Component Catalog and Fail-Closed Adapter

`[[projects/manifesto/manifesto]]` treats components as external capabilities rather than local feature modules. The runtime expresses that through `ComponentServiceClient`, which validates types through an HTTP component catalog and now fails closed when that dependency is unavailable or unhealthy.

## Key Ideas

- `ComponentUseCaseImpl::add_component()` refuses to attach a component until `validate_component_type()` succeeds and the project does not already contain the same component type.
- `ComponentServiceClient` calls `GET {base_url}/api/components` and parses `ComponentInfo` values when the external service responds successfully.
- `service.component_service.base_url`, `api_key`, and `timeout_seconds` are all consumed by the live runtime.
- When the HTTP call fails or returns a non-success status, the adapter returns `DomainError::ExternalServiceError`; it does not fall back to built-in mock component data.
- `tests/component_service_client_tests.rs` covers both the fail-closed error path and bearer API-key forwarding.
- `ComponentResponse.endpoint` and `access_token` still remain unset, which means type validation exists today but full provisioning handoff does not.

## Open Questions

- Should endpoint discovery and component-scoped token issuance come from the same adapter later, or from a separate provisioning flow?

## Sources

- [[projects/manifesto/manifesto]] - Service overview for Manifesto's component orchestration role.
- [[projects/manifesto/concepts/component-instance-permissions]] - Permission and resource model that accompanies component attachment.
- [[projects/manifesto/references/manifesto-api-and-permission-flows]] - API flows that expose add/list/remove component behavior.
- [[projects/manifesto/references/manifesto-event-model]] - Event behavior that accompanies component status changes.
