---
title: Organization-Resource Authorization
category: concepts
tags: [authorization, permissions, organizations, openfga, visibility/internal]
sources:
  - Hive/http/src/lib.rs
  - Hive/setup/src/app.rs
  - hive-events/src/organization.rs
  - hive-events/src/member.rs
  - openfga/model.fga
  - sentinel-sync/src/translator/hive.rs
summary: Hive no longer owns its authorization rules. Organization, member, and external-link permissions are derived from relation tuples in OpenFGA. Hive emits lifecycle events, sentinel-sync translates them into tuples, and the route layer only asks Check.
updated: 2026-04-20
---

# Organization-Resource Authorization

Hive's authorization model is now fully declarative. Every permission question collapses to a `Check` call against the central OpenFGA store on the `organization` type.

## Emitted tuples

Hive emits domain events that the [[projects/sentinel-sync/sentinel-sync]] worker translates into tuples:

| Hive event             | Tuple writes                                                                               | Tuple deletes                                                             |
|------------------------|--------------------------------------------------------------------------------------------|---------------------------------------------------------------------------|
| `OrganizationCreated`  | `organization:{id}#owner@user:{owner_user_id}`                                             | —                                                                         |
| `MemberJoined`         | `organization:{id}#member@user:{user_id}` plus one role-relation tuple per `Role.permission` | —                                                                         |
| `MemberRemoved`        | —                                                                                          | every `organization:{id}#{relation}@user:{user_id}` for `owner/admin/member/viewer` |
| `OrganizationUpdated` / `OrganizationDeleted` / `MemberInvited` / `MemberRolesUpdated` / `ExternalLinkCreated` / `SyncJob*` | see [[projects/sentinel-sync/references/event-to-tuple-mapping]] | ditto |

Role string to OpenFGA relation mapping (from `sentinel-sync/src/translator/hive.rs`):

| Hive `Role.permission` | OpenFGA relation on `organization` |
|------------------------|------------------------------------|
| `owner`                | `owner`                            |
| `admin`                | `admin`                            |
| `write`                | `member`                           |
| `read`                 | `viewer`                           |

## Route guards

Every guarded Hive route uses `with_permission_on(Permission::X, "organization")` — `member`, `external_link`, and sync-job routes collapse to organization-level checks because the old `.conf` files already treated sub-resources as unidentified. The deepest UUID in the path is always the resource instance that OpenFGA resolves against.

See [Hive/http/src/lib.rs](../../../../../Hive/http/src/lib.rs) for the full route surface.

## What went away

- `Hive/resources/permissions/*.conf` — deleted.
- `Hive/domain/src/service/permission_service.rs` (`ResourcePermissionFetcher`) — deleted.
- Any `PermissionsFetcher` parameters on `create_app_routes` and `Application::new` — removed.

## Sources

- [[projects/hive/hive]]
- [[projects/sentinel-sync/references/event-to-tuple-mapping]]
- [[projects/sentinel-sync/references/openfga-model]]
- [[projects/rustycog/references/rustycog-permission]]
- [[concepts/openfga-as-authorization-engine]]
