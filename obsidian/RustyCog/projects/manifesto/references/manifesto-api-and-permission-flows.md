---
title: Manifesto API and Permission Flows
category: references
tags: [reference, api, permissions, openfga, visibility/internal]
sources:
  - Manifesto/http/src/lib.rs
  - Manifesto/setup/src/app.rs
  - Manifesto/application/src/command/factory.rs
  - Manifesto/application/src/usecase/project.rs
  - Manifesto/application/src/usecase/component.rs
  - Manifesto/application/src/usecase/member.rs
  - manifesto-events/src/lib.rs
  - sentinel-sync/src/translator/manifesto.rs
  - openfga/model.fga
summary: >-
  Manifesto-specific API behavior on top of RustyCog's shared HTTP shell, plus the OpenFGA-backed authorization model that replaced the per-resource fetcher pattern.
updated: 2026-04-20
---

# Manifesto API and Permission Flows

This page assumes the shared [[projects/rustycog/references/rustycog-http]] and [[concepts/centralized-authorization-service]] patterns are already familiar. It keeps the route, command, and authorization details that are specific to [[projects/manifesto/manifesto]].

## RustyCog Baseline

- [[projects/rustycog/references/rustycog-http]] explains `RouteBuilder`, authentication modes, command-context extraction, and the centralized permission middleware.
- [[concepts/centralized-authorization-service]] explains why every check goes through one shared `Arc<dyn PermissionChecker>` and how tuples reach OpenFGA via [[projects/sentinel-sync/sentinel-sync]].
- [[projects/rustycog/references/rustycog-command]] covers the shared command execution runtime that the handlers delegate into.

## Service-Specific Differences

- [Manifesto/http/src/lib.rs](../../../../../Manifesto/http/src/lib.rs) registers project, component, and member routes against the same shared `permission_checker` on `AppState`. There is no per-resource fetcher anymore.
- Project get/detail routes are `.might_be_authenticated().with_permission_on(Permission::Read, "project")`. As of Phase 1 of [[concepts/anonymous-public-read-via-wildcard-subject]] (2026-04-22), `optional_permission_middleware` resolves anonymous callers as `Subject::wildcard()` and consults the centralized OpenFGA checker — but only `viewer@user:*` tuples grant access, and **`sentinel-sync` does not write those yet** (Phase 2 follow-up). So in production today, anonymous reads of a specific project still 403; authenticated reads work as long as the calling user has any of `owner` / `admin` / `member` / `viewer` on the project (or inherits one from its organization). Public-project access end-to-end is unblocked once Phase 2 ships the `ProjectVisibilityChanged` event and the matching `sentinel-sync` translator arms.
- `GET /api/projects` is also optionally authenticated, but its visibility enforcement happens through command, use-case, service, and repository filtering rather than the UUID-scoped permission middleware used by get/detail routes.
- Component routes use `"project"` as the OpenFGA object type today because the deepest UUID in component routes is the project id (`{component_type}` is a string segment). When component routes adopt `{component_id}` UUID parameters, switch the relevant routes to `with_permission_on(_, "component")`.
- Member routes are project-scoped (`with_permission_on(Permission::Admin, "project")`).
- Permission grant/revoke endpoints emit `PermissionGrantedEvent` / `PermissionRevokedEvent`. The Manifesto translator maps the string `resource` to either `project` or `component` and writes/deletes the matching relation tuple — see [[projects/sentinel-sync/references/event-to-tuple-mapping]].
- `ComponentUseCaseImpl` keeps domain state and emitted events synchronized so the OpenFGA tuple graph stays consistent.
- `ProjectDetailResponse` and `ComponentResponse` still leave `endpoint` and `access_token` as `None`, so the API currently exposes component attachment metadata rather than a provisioning handoff.

## Open Questions

- Should Manifesto eventually surface a richer operator-facing story for component provisioning and component-scoped tokens?
- Should component routes adopt UUID `{component_id}` parameters so the middleware can guard against `"component"` directly?

## Sources

- [[projects/manifesto/manifesto]]
- [[projects/manifesto/concepts/project-ownership-and-publication-lifecycle]]
- [[projects/manifesto/concepts/component-instance-permissions]]
- [[concepts/centralized-authorization-service]]
- [[concepts/openfga-as-authorization-engine]]
- [[projects/sentinel-sync/references/event-to-tuple-mapping]]
- [[projects/manifesto/references/manifesto-event-model]]
- [[concepts/anonymous-public-read-via-wildcard-subject]] — wildcard-subject design and the Phase 2 hand-off blocking anonymous public-read.
