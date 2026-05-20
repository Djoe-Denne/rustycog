---
title: >-
  Manifesto
category: project
tags: [projects, orchestration, blueprint, visibility/internal]
sources:
  - Manifesto/README.md
  - Manifesto/SETUP.md
  - Manifesto/IMPLEMENTATION_STATUS.md
  - Manifesto/src/main.rs
  - Manifesto/setup/src/app.rs
  - Manifesto/http/src/lib.rs
  - Manifesto/application/src/command/factory.rs
  - Manifesto/configuration/src/lib.rs
  - Manifesto/tests/common.rs
  - Manifesto/docs/rustycog-service-build-guide.md
  - Manifesto/docs/rustycog-hexagonal-web-service-guide.md
  - Manifesto/docs/rustycog-implementation-and-usage-guide.md
summary: >-
  Manifesto is AIForAll's project-service and a practical RustyCog variant, with this page focused
  on orchestration-specific behavior and the now-aligned runtime/docs boundary after remediation.
provenance:
  extracted: 0.82
  inferred: 0.12
  ambiguous: 0.06
created: 2026-04-14T16:54:59.5971424Z
updated: 2026-04-19T18:00:00Z
---

# Manifesto

## Indexes

- [[projects/manifesto/concepts/index]] — concepts
- [[projects/manifesto/skills/index]] — skills
- [[projects/manifesto/references/index]] — references

Manifesto is the project-management service for AIForAll. Use `[[projects/rustycog/references/index]]` for the shared service shell and crate behavior; use this page and the linked Manifesto references for the project-domain rules, service-specific wiring, and current runtime truth.

## RustyCog Baseline

- `[[projects/rustycog/references/index]]` is the canonical map for the shared command, config, HTTP, permissions, DB, event, and testing crates that Manifesto composes.
- `[[references/rustycog-service-construction]]` and `[[skills/building-rustycog-services]]` describe the default RustyCog service assembly flow that Manifesto follows with project-specific additions.
- Read `[[projects/rustycog/references/rustycog-command]]`, `[[projects/rustycog/references/rustycog-config]]`, `[[projects/rustycog/references/rustycog-http]]`, `[[projects/rustycog/references/rustycog-permission]]`, and `[[projects/rustycog/references/rustycog-testing]]` for the shared baseline that this project specializes.

## Service-Specific Differences

- Manifesto treats projects as assemblies of independently implemented components with their own lifecycle, visibility, and configuration flow.
- The composition root is recognizably RustyCog-shaped, but Manifesto adds project-, component-, and member-scoped permission fetchers plus its own `ManifestoCommandRegistryFactory`.
- Live runtime now wires verified HS256-only auth, logging level, command retry, component-service timeout/api key, and business limits from config instead of leaving those knobs as guide-era leftovers.
- Public project/component resource reads use optional-auth routes plus explicit-anonymous permission evaluation; private reads still require real access.
- `GET /api/projects` uses optional auth plus visibility filtering in the service/repository layers rather than the UUID-scoped permission middleware used by item/detail routes.
- Component catalog integration is fail-closed.
- Component add/remove now treats component-instance ACL sync as part of the same consistency boundary and fails instead of silently drifting state.
- Apparatus status consumption is wired into startup when queue config resolves to a real consumer, while checked-in local/test configs keep queues disabled by default.
- `ComponentResponse.endpoint` and `access_token` still remain unset, so component provisioning handoff is the main product boundary that is still deliberately narrow.

## Related

- [[projects/rustycog/references/index]] - Canonical shared framework map that the service pages below build on.
- [[references/rustycog-service-construction]] - Generic RustyCog construction flow that Manifesto specializes.
- [[projects/manifesto/concepts/project-ownership-and-publication-lifecycle]] - Ownership bootstrap, defaults, and publish/archive transitions.
- [[projects/manifesto/concepts/component-instance-permissions]] - Generic versus per-instance component permission model.
- [[projects/manifesto/concepts/component-catalog-and-fallback-adapter]] - External component catalog integration and fail-closed behavior.
- [[projects/manifesto/references/manifesto-entity-model]] - Project, component, membership, and project-scoped RBAC entities.
- [[projects/manifesto/references/manifesto-api-and-permission-flows]] - Live route and permission behavior.
- [[projects/manifesto/references/manifesto-event-model]] - Events emitted and consumed by the service.
- [[projects/manifesto/references/manifesto-runtime-and-configuration]] - `MANIFESTO_*` config loading, queue posture, and runtime wiring.
- [[projects/manifesto/references/manifesto-testing-and-fixtures]] - DB-backed API harness plus focused runtime/auth/client tests.
- [[projects/manifesto/skills/extending-manifesto-project-service]] - Practical workflow for adding commands, routes, permissions, events, and tests.

## Open Questions

- When Manifesto eventually exposes richer component provisioning, should that happen through the existing component catalog boundary or through a separate runtime handoff flow?
- If queue-backed operation becomes more common outside local/test, should the checked-in config examples start surfacing explicit broker settings?

## Sources

- [[projects/manifesto/references/manifesto-service]] — Product model, runtime wiring, and project-service ADR summary
- [[projects/rustycog/references/index]] — Shared crate-level baseline for the runtime this service specializes
- [[references/rustycog-service-construction]] — Manifesto-authored RustyCog build and wiring guides
- [[skills/building-rustycog-services]] — Practical workflow distilled from those guides
