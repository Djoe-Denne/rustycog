---
title: Hive Data Model and Schema
category: references
tags: [reference, schema, organizations, visibility/internal]
sources:
  - Hive/infra/src/repository/entity/hive_database_schema.sql
  - Hive/domain/src/service/permission_service.rs
  - Hive/domain/src/service/external_provider_service.rs
  - Hive/domain/src/service/sync_service.rs
summary: Hive persists organizations, members, invitations, external links, sync jobs, and permission resources in a schema that mirrors its service boundaries.
provenance:
  extracted: 0.81
  inferred: 0.11
  ambiguous: 0.08
created: 2026-04-14T18:56:22.3888182Z
updated: 2026-04-14T18:56:22.3888182Z
---

# Hive Data Model and Schema

These sources show how `[[projects/hive/hive]]` persists its organization-management domain: organizations and their members, invitations and roles, external provider links, sync jobs, and the resource-permission model that backs route authorization.

## Key Ideas

- `organizations` stores core org identity plus `settings JSONB`, while `organization_members` and `organization_roles` model membership and role assignment.
- `organization_invitations` stores token, status, expiry, inviter, message, and role linkage, which matches Hive's invitation-first onboarding flow.
- `external_providers` catalogs supported provider sources, `external_links` stores org-to-provider linkage and sync settings, and `sync_jobs` records execution status, counters, timestamps, and error details.
- The permission model is explicit: `permissions`, `resources`, and `role_permissions` turn organization role membership into resource-scoped permission checks for HTTP routes.
- Schema comments and service code make it clear that sync jobs, external links, and permission tables are not bolt-ons; they are part of Hive's core product model.
- Conflict to resolve: the schema is rich enough for role CRUD and broader external-link lifecycle management, while the live HTTP surface currently exposes only a subset of that model. ^[ambiguous]

## Open Questions

- The schema and domain services imply richer lifecycle management than the current live route builder exposes, so some tables may be ahead of the shipped API.
- This source set shows the SQL narrative clearly, but not a single up-to-date ER diagram or service-owned terminology page for operators. ^[inferred]

## Sources

- [[projects/hive/hive]] - Service that owns this data model.
- [[projects/hive/concepts/organization-resource-authorization]] - Permission checks built on top of these tables.
- [[projects/hive/concepts/external-provider-sync-jobs]] - External-link and sync-job flows built on the schema.
- [[projects/hive/references/hive-http-api-and-openapi-drift]] - API surface that only partially exposes this model.