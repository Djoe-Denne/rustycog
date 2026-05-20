---
title: Telegraph HTTP and Notification API
category: references
tags: [reference, api, notifications, visibility/internal]
sources:
  - Telegraph/openspecs.yaml
  - Telegraph/http/src/lib.rs
  - Telegraph/http/src/handlers/notification.rs
  - Telegraph/http/src/handlers/communication.rs
  - Telegraph/setup/src/app.rs
  - Telegraph/application/src/usecase/notification.rs
  - Telegraph/domain/src/service/notification_service.rs
  - openfga/model.fga
summary: Telegraph-specific HTTP behavior layered on top of RustyCog's shared route and permission model, including its notification-only live surface and ownership checks.
provenance:
  extracted: 0.72
  inferred: 0.14
  ambiguous: 0.14
created: 2026-04-14T18:18:24.0602572Z
updated: 2026-04-19T12:08:26.9393504Z
---

# Telegraph HTTP and Notification API

This page assumes the shared `[[projects/rustycog/references/rustycog-http]]` routing shell and permission model. It keeps the live HTTP behavior that is specific to `[[projects/telegraph/telegraph]]`.

## RustyCog Baseline

- `[[projects/rustycog/references/rustycog-http]]` explains `RouteBuilder`, authenticated routes, command-context propagation, and shared error mapping.
- `[[concepts/centralized-authorization-service]]` explains the OpenFGA-backed `PermissionChecker` that Telegraph now delegates to.

## Service-Specific Differences

- The OpenAPI file documents `GET /api/notifications`, `GET /api/notifications/unread-count`, `PUT /api/notifications/{id}/read`, and `GET /health` as the live Telegraph surface.
- `http/src/lib.rs` wires those three notification endpoints through `rustycog_http::RouteBuilder`, marks them authenticated, and uses `.with_permission_on(Permission::Write, "notification")` on the mark-read route.
- Notification handlers build typed commands, attach `CommandContext::with_user_id()`, and map validation, business, not-found, unauthorized, and internal failures into HTTP status codes.
- `NotificationUseCaseImpl` handles pagination defaults, `per_page <= 100`, unread filtering, and response shaping for the notification read model.
- Ownership is enforced twice: the centralized `PermissionChecker` resolves `notification:{id}#recipient@user:{user_id}` against OpenFGA (tuples are written by [[projects/sentinel-sync/sentinel-sync]] when Telegraph starts publishing `NotificationCreated`), and `NotificationServiceImpl::mark_notification_as_read()` still returns an unauthorized domain error if the record belongs to someone else.
- Until Telegraph wires its notification outbox into the queue, the OpenFGA store has no recipient tuples and the route layer falls back to the in-domain ownership check inside `NotificationServiceImpl`. ^[ambiguous]
- `http/src/handlers/communication.rs` defines richer direct-send DTOs for email, notification, and SMS payloads, but the live route table does not register those handlers. ^[ambiguous]
- The unregistered direct-send DTOs are best treated as future product surface, not as the default extension path for new event-driven delivery work; Telegraph's live feature additions belong on the queue path unless the HTTP contract intentionally changes. ^[inferred]

## Open Questions

- The OpenAPI contract presents Telegraph as a notification service with real-time SQS processing, while the live HTTP server exposes only the read-model half of that broader story. ^[ambiguous]
- `RouteBuilder::health_check()` likely provides the `/health` endpoint described in OpenAPI, but the service does not define a dedicated Telegraph-specific health handler in this crate. ^[inferred]

## Sources

- [[projects/telegraph/telegraph]] - Project page for the service exposing these routes.
- [[projects/telegraph/concepts/multi-channel-delivery-modes]] - Broader communication DTOs versus the live notification-only routes.
- [[projects/telegraph/references/telegraph-service]] - Service shape and `rustycog_http` routing context.
- [[projects/rustycog/references/rustycog-http]] - `RouteBuilder`, `AppState`, and permission middleware used by the live server.
- [[projects/telegraph/references/telegraph-testing-and-smtp-fixtures]] - Integration tests that exercise the live API.
