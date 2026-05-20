---
title: Invitation-Driven Membership
category: concepts
tags: [organizations, invitations, membership, visibility/internal]
sources:
  - Hive/application/src/usecase/invitation.rs
  - Hive/application/src/command/factory.rs
  - Hive/http/src/handlers/invitations.rs
  - Hive/openspecs.yaml
  - hive-events/README.md
summary: Hive models invitations as tokenized membership objects with roles, expiry, and event emission for downstream notification flows.
provenance:
  extracted: 0.74
  inferred: 0.14
  ambiguous: 0.12
created: 2026-04-14T18:56:22.3888182Z
updated: 2026-04-19T11:13:11Z
---

# Invitation-Driven Membership

`[[projects/hive/hive]]` treats invitations as first-class organization objects rather than as a side effect of adding a member directly. That lets Hive attach role permissions, expiry, invitee identity, and messaging context to the onboarding flow, then publish an event for downstream notification handling.

## Key Ideas

- `InvitationUseCaseImpl::create_invitation()` creates an invitation by email, converts requested roles into domain role-permission structures, and returns a token-bearing invitation response.
- The use case publishes `HiveDomainEvent::InvitationCreated` with organization name, invitation ID, invitee email, role list, inviter, token, and expiry so downstream systems can notify the recipient.
- Hive's command registry registers create, list, cancel, accept, get-by-token, and resend invitation commands, which means the command layer supports more invitation operations than the live HTTP router currently exposes.
- `http/src/handlers/invitations.rs` contains handlers for list, get, accept, and token-based lookup, but `http/src/lib.rs` currently wires only `POST /api/organizations/{organization_id}/invitations`. Conflict to resolve. ^[ambiguous]
- `openspecs.yaml` goes even further, documenting public token lookup, accept, and decline routes under `/api/invitations/{token}`, so the declared contract, handler set, and registered routes currently form three different invitation stories. Conflict to resolve. ^[ambiguous]
- `[[projects/hive-events/hive-events]]` explicitly frames invitation events as notification-triggering events for Telegraph, so invitation handling is part of the platform's event choreography, not just a CRUD subresource.

## Open Questions

- The OpenAPI contract documents a decline path, but the current command registry and handler set do not show a dedicated decline command. Conflict to resolve. ^[ambiguous]
- The live test suite in this source batch does not yet show invitation API coverage end to end, so the exact shipped invitation surface still needs runtime confirmation. ^[ambiguous]

## Sources

- [[projects/hive/hive]] - Service where invitations drive membership onboarding.
- [[projects/hive-events/hive-events]] - Event contract crate that carries invitation notifications downstream.
- [[projects/hive/references/hive-http-api-and-openapi-drift]] - HTTP and spec mismatch around invitation routes.
- [[projects/hive/references/hive-command-execution]] - Command-level invitation operations.