---
title: Extending Manifesto Project Service
category: skills
tags: [projects, services, openfga, permissions, visibility/internal]
sources:
  - Manifesto/setup/src/app.rs
  - Manifesto/application/src/command/factory.rs
  - Manifesto/http/src/lib.rs
  - Manifesto/application/src/usecase/project.rs
  - Manifesto/application/src/usecase/component.rs
  - Manifesto/application/src/usecase/member.rs
  - manifesto-events/src/lib.rs
  - sentinel-sync/src/translator/manifesto.rs
  - openfga/model.fga
  - Manifesto/tests/common.rs
summary: >-
  Workflow for adding a new Manifesto capability by threading commands, routes, OpenFGA-backed permission guards, events, and tests through the existing project-service shape.
updated: 2026-04-20
---

# Extending Manifesto Project Service

Use this page when adding a new capability to [[projects/manifesto/manifesto]]. The practical path is not "just add a handler": Manifesto usually threads one change through entities, use cases, commands, routes, OpenFGA tuples, events, and tests together.

## Workflow

- Decide the scope first: does the change belong under the existing `project`, `component`, or `member` surfaces, or does it justify a genuinely new resource boundary?
- If the change adds or reshapes persisted state, update the domain entity, repository flow, migration, and DB fixtures together so the write path and the tests stay aligned.
- Add or update the use case first, then expose it through a command and handler pair; keep `command_type()` aligned with the registration key in `ManifestoCommandRegistryFactory`.
- Wire the route in [Manifesto/http/src/lib.rs](../../../../../Manifesto/http/src/lib.rs) through `RouteBuilder`, choosing the auth mode and `.with_permission_on(Permission::X, "<openfga_type>")` immediately after it. Acceptable object types today: `"project"`, `"component"`. The middleware always scopes the check to the deepest UUID in the request path.
- If the change introduces an authorization fact (a new role, a new resource type, a new derived relation), update [openfga/model.fga](../../../../../openfga/model.fga) and re-upload the model to OpenFGA before shipping.
- Add or extend the matching event variant in `manifesto-events`, then add the corresponding arm to `sentinel-sync/src/translator/manifesto.rs` so the OpenFGA store stays in sync. Update [[projects/sentinel-sync/references/event-to-tuple-mapping]] in the same change.
- Publish a Manifesto domain event from the use case when the change matters outside the local transaction (for both Telegraph notifications and sentinel-sync tuple writes). The current service treats publication as best effort and logs failures instead of aborting the main write.
- Close the loop with API tests through `setup_test_server()`, real JWTs, DB fixtures, and an `InMemoryPermissionChecker` seeded with the expected tuples; cover both the happy path and at least one permission-denied or invalid-state case.

## Common checks

- If you add a new config knob, verify that `src/main.rs` or `setup/src/app.rs` actually consumes it rather than assuming the presence of a TOML key makes it live.
- If you add or extend component-facing behavior, remember that the current project-detail response still returns `endpoint` and `access_token` as `None`; do not assume Manifesto already owns runtime handoff to the component service.
- If you add queue-sensitive behavior, the default test harness will not exercise SQS for you; add targeted coverage instead of relying on the standard API suites alone.
- If you add a translator arm, run `cargo test -p sentinel-sync` to confirm the deterministic mapping. Pair every "create" arm with a matching "delete" arm so reverse events are idempotent.

## Sources

- [[projects/manifesto/manifesto]]
- [[projects/manifesto/references/manifesto-api-and-permission-flows]]
- [[projects/manifesto/references/manifesto-runtime-and-configuration]]
- [[projects/manifesto/references/manifesto-event-model]]
- [[projects/manifesto/references/manifesto-testing-and-fixtures]]
- [[projects/manifesto/concepts/project-ownership-and-publication-lifecycle]]
- [[projects/manifesto/concepts/component-instance-permissions]]
- [[projects/manifesto/concepts/component-catalog-and-fallback-adapter]]
- [[projects/sentinel-sync/references/event-to-tuple-mapping]]
- [[projects/sentinel-sync/skills/extending-sentinel-sync-with-new-events]]
- [[skills/building-rustycog-services]]
