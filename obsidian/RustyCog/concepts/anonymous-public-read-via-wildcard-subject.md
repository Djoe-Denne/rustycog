---
title: Anonymous Public-Read via Wildcard Subject
category: concepts
tags: [concept, permissions, openfga, public-read, visibility/internal]
sources:
  - rustycog/rustycog-permission/src/lib.rs
  - rustycog/rustycog-permission/src/checker.rs
  - rustycog/rustycog-http/src/middleware_permission.rs
  - rustycog/rustycog-testing/src/permission/service.rs
  - openfga/model.fga
  - sentinel-sync/src/translator/manifesto.rs
  - manifesto-events/src/project.rs
  - Manifesto/application/src/usecase/project.rs
summary: How anonymous read of public projects flows through the centralized OpenFGA checker via a wildcard `user:*` subject — Phase 1 plumbs the shared crates and OpenFGA model; Phase 2 (pending) wires `sentinel-sync` to write the corresponding tuples on visibility changes.
provenance:
  extracted: 0.62
  inferred: 0.30
  ambiguous: 0.08
created: 2026-04-22T18:30:00Z
updated: 2026-04-22T18:30:00Z
---

# Anonymous Public-Read via Wildcard Subject

The platform's authorization story is "every decision goes through the centralized [[concepts/openfga-as-authorization-engine]] checker, no per-route bypass." That sentence works cleanly for authenticated callers, but until 2026-04-22 it had a hard limit: the `optional_permission_middleware` rejected anonymous callers with `403 FORBIDDEN` whenever the request path carried a resource UUID, **before** consulting the checker. Public-read of a specific project worked only in `tests/public_acl_api_tests.rs` unit tests against the read repository — never end-to-end through HTTP.

This page documents the **wildcard subject pattern** that closes the gap, what's already shipped (Phase 1), and what's still missing (Phase 2).

## The wildcard subject

`[[projects/rustycog/references/rustycog-permission]]` ships a `Subject` that can model the special "any user" subject:

```rust
pub enum SubjectKind { User, Wildcard }

pub struct Subject {
    pub user_id: Uuid,
    pub kind: SubjectKind,
}

impl Subject {
    pub fn new(user_id: Uuid) -> Self { /* User */ }
    pub fn wildcard() -> Self { /* Wildcard, user_id: Uuid::nil() */ }
}

impl Display for Subject {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.kind {
            SubjectKind::Wildcard => write!(f, "user:*"),
            SubjectKind::User => write!(f, "user:{}", self.user_id),
        }
    }
}
```

`user:*` is the OpenFGA wire form for "any user". Combined with a model relation declared as `[user, user:*]`, a single tuple `project:{id}#viewer@user:*` grants every caller (authenticated or not) the `viewer` relation on that project — and `viewer` derives `read` per the model.

## Phase 1 — what's shipped today (2026-04-22)

```mermaid
flowchart LR
    Anon["Anonymous request"] --> Mw["optional_permission_middleware"]
    Mw -- "Subject::wildcard()" --> Checker["OpenFgaPermissionChecker.check"]
    Checker -- 'POST /check { user: "user:*" }' --> Fga["OpenFGA store"]
    Fga -- "no viewer@user:* tuple yet" --> Deny["allowed: false -> 403"]
    Fga -. "with Phase 2 tuple" .-> Allow["allowed: true -> 200"]
```

The four shared-layer changes:

1. **`Subject::wildcard()` constructor and `SubjectKind` discriminant** in `rustycog-permission`. The struct shape is preserved (existing `Subject::new(uuid)` call sites in Hive, Telegraph, IAMRusty, Manifesto are untouched). `#[serde(default)]` on the new `kind` field keeps wire compatibility with payloads serialized before the field existed.
2. **`CachedPermissionChecker` bypasses the cache for wildcard subjects.** The cache key is `(user_id, permission, object_type, object_id)` — wildcard reuses `Uuid::nil()`, which would collide across every anonymous request and let one project's public-read decision answer for another. Skipping the cache also means a public→private flip (when sentinel-sync removes the wildcard tuple in Phase 2) is observed on the very next request rather than after the TTL window. ^[inferred]
3. **`optional_permission_middleware` consults the checker with `Subject::wildcard()`** instead of short-circuiting with 403 on missing JWT. Fail-closed semantics are preserved: relations without a wildcard tuple still return `allowed: false` and the request 403s.
4. **OpenFGA model** declares `project.viewer: [user, user:*] or member or viewer from organization` so the store will accept `viewer@user:*` writes.

The `[[projects/rustycog/references/openfga-mock-service]]` test fake gained `mock_check_allow_wildcard(action, resource)` and `mock_check_deny_wildcard(action, resource)` helpers so test suites can arrange anonymous-read decisions without constructing `Subject::wildcard()` themselves.

### Why nothing changes in production yet

No service writes `viewer@user:*` tuples today. `sentinel-sync` translates Manifesto's `ProjectCreated` to `owner` and (optional) `organization` tuples but ignores `evt.visibility`, and `ProjectUpdated` is a no-op for tuples. So the production OpenFGA store has zero `user:*` tuples, every wildcard `Check` returns `allowed: false`, and anonymous requests still 403 — but now they 403 *after* a deliberate checker decision instead of before, which is the right shape for Phase 2 to flip on.

## Phase 2 — what's still missing

The cross-service plumbing needed to actually unlock anonymous public-read:

1. **New `ProjectVisibilityChangedEvent`** in `manifesto-events/src/project.rs`:
   ```rust
   pub struct ProjectVisibilityChangedEvent {
       pub base: BaseEvent,
       pub project_id: Uuid,
       pub old_visibility: String,
       pub new_visibility: String,
       pub changed_by: Uuid,
       pub changed_at: DateTime<Utc>,
   }
   ```
   Matches the existing granular-event style (`ProjectPublished`, `ProjectArchived`, `ComponentStatusChanged` are all dedicated despite being "just" state changes). Carries old + new in the payload so sentinel-sync can react idempotently.
2. **Manifesto's `update_project` use case** emits `ProjectVisibilityChanged` (in addition to the generic `ProjectUpdated`) when visibility actually flips.
3. **`sentinel-sync` translator updates** in `sentinel-sync/src/translator/manifesto.rs`:
   - `ProjectCreated` with `evt.visibility == "public"` → write `project:{id}#viewer@user:*`.
   - New `ProjectVisibilityChanged` arm → write or delete the wildcard tuple based on `(old, new)`. See [[projects/sentinel-sync/references/event-to-tuple-mapping]] for the row stub.
   - `ProjectDeleted` → sweep all tuples on `project:{id}` (already noted as a TODO in the existing translator at `manifesto.rs:169-172`).
4. **Extend `Tuple::user`** in `sentinel-sync/src/fga_client.rs` to accept either a `Uuid` or the `*` wildcard.
5. **Revert the 3 Phase 1 test authentications** in `Manifesto/tests/project_api_tests.rs` back to anonymous and arrange `openfga.mock_check_allow_wildcard(Permission::Read, project_resource)` in `setup_test_server` (or per-test).
6. **Add a true end-to-end public-read test** that creates a public project, asserts the `viewer@user:*` tuple gets written through the wiremock fake's inspection helpers, then issues an anonymous GET and asserts `200`.
7. **Production data backfill** for existing public projects (out of code scope; ops coordination doc).

## Cleanup invariants (the hard part)

The user concern that prompted this plan: "when a public project becomes private, who removes the tuple?" The answer in Phase 2 is `sentinel-sync`, but it has to be careful about three failure modes:

- **Crash between DB write and event publish.** Manifesto's `update_project` writes the DB row first, then publishes the event. If the publish fails after the DB succeeds, the OpenFGA tuple stays out of sync. ^[ambiguous] Telegraph and IAMRusty face the same risk on their own events; the platform-wide answer should be a periodic reconciler that diffs DB visibility against OpenFGA wildcard tuples and corrects drift.
- **Race between two concurrent `update_project` calls.** Both publish events; sentinel-sync sees them in some order. As long as `(old, new)` is in the payload, the second event's translation is "idempotent in terms of the final state" — the last-applied flips OpenFGA to whatever the last DB write was. The dedicated event payload is what makes this work; an enriched `ProjectUpdatedEvent` with only `updated_fields: ["visibility"]` would not. ^[inferred]
- **Replay of a `ProjectVisibilityChanged` event.** The translator must be safe to apply twice. `Write` and `Delete` against OpenFGA are idempotent (writing an existing tuple is a no-op; deleting an absent tuple errors but can be swallowed). ^[inferred]

## Not in scope (Phase 1 or 2)

- **`component.viewer` wildcard.** The model derives `component.viewer` from `member from project`, so a project's `viewer@user:*` doesn't currently propagate to its components. A separate model edit + sentinel-sync change is needed if Manifesto wants public-component-read on private projects.
- **`organization.viewer` wildcard.** No use case in any service today.
- **`Visibility::Internal` semantics.** Means "anyone in the same org", which is a different tuple shape (`viewer@organization:{id}#member`) and a separate design conversation.

## Sources

- [[projects/rustycog/references/rustycog-permission]] — `Subject::wildcard()` and the cache bypass.
- [[projects/rustycog/references/openfga-mock-service]] — `mock_check_*_wildcard` helpers.
- [[projects/rustycog/references/wiremock-mock-server-fixture]] — singleton listener that the wildcard tests share.
- [[projects/manifesto/references/manifesto-api-and-permission-flows]] — Manifesto routes that depend on this work.
- [[projects/manifesto/references/manifesto-testing-and-fixtures]] — current test wiring + the temporary auth on the 3 GET tests.
- [[projects/sentinel-sync/references/event-to-tuple-mapping]] — Phase 2 translator rows.
- [[concepts/openfga-as-authorization-engine]] — surrounding architecture.
- [[concepts/centralized-authorization-service]] — the contract this pattern satisfies.
