---
title: Telegraph
category: project
tags: [communication, events, notifications, visibility/internal]
sources:
  - README.md
  - Telegraph/openspecs.yaml
  - Telegraph/setup/src/app.rs
  - Telegraph/infra/src/event/consumer.rs
  - Telegraph/http/src/lib.rs
summary: Telegraph is a Rust communication service whose docs now treat RustyCog as the shared baseline and focus here on the queue, descriptor, and notification behaviors unique to Telegraph.
provenance:
  extracted: 0.73
  inferred: 0.17
  ambiguous: 0.10
created: 2026-04-14T18:18:24.0602572Z
updated: 2026-04-19T12:08:26.9393504Z
---

# Telegraph

## Indexes

- [[projects/telegraph/concepts/index]] — concepts
- [[projects/telegraph/skills/index]] — skills
- [[projects/telegraph/references/index]] — references

`Telegraph` is the communication service in the AIForAll workspace. Use `[[projects/rustycog/references/index]]` for the shared service shell; use this page and the linked Telegraph references for the event-routing, descriptor, delivery, and notification behaviors that Telegraph adds on top of that baseline.

## RustyCog Baseline

- `[[projects/rustycog/references/index]]` is the canonical map for the shared command, config, HTTP, permission, event, DB, and testing crates used by Telegraph.
- `[[references/rustycog-service-construction]]` and `[[skills/building-rustycog-services]]` describe the default RustyCog service assembly flow that Telegraph reuses.
- Read `[[projects/rustycog/references/rustycog-command]]`, `[[projects/rustycog/references/rustycog-config]]`, `[[projects/rustycog/references/rustycog-events]]`, `[[projects/rustycog/references/rustycog-http]]`, and `[[projects/rustycog/references/rustycog-testing]]` for the shared runtime mechanics.

## Service-Specific Differences

- Telegraph runs two entry paths in parallel: an SQS-backed consumer that reacts to IAM events and an authenticated HTTP API that serves stored notification state.
- Event payloads are transformed into user-facing output through `[[projects/telegraph/concepts/descriptor-driven-communications]]`, where per-event TOML descriptors and Tera templates determine whether an event yields email, in-app notification content, or both.
- Queue events flow through `[[projects/telegraph/concepts/queue-driven-command-processing]]`, so async consumers and HTTP handlers both delegate into the same RustyCog command runtime instead of bypassing it.
- The notification API follows the shared `[[concepts/centralized-authorization-service]]` pattern: the route layer asks the OpenFGA-backed `PermissionChecker` whether the caller is a `notification#recipient`. Tuples are written by [[projects/sentinel-sync/sentinel-sync]] when Telegraph starts publishing `NotificationCreated` events.
- Runtime behavior depends on `[[concepts/structured-service-configuration]]`, especially the split between transport-level `queue` settings and Telegraph-specific `queues.*` event routing and `communication.*` delivery settings.
- The root repo overview and some Telegraph config/model surfaces still describe SMS alongside email and notifications, but the currently wired processor composite only registers email and notification handlers. ^[ambiguous]

## Related

- [[projects/rustycog/references/index]] - Canonical shared framework map that the service pages below build on.
- [[projects/telegraph/references/telegraph-service]] - Code-backed overview of Telegraph's crate layout, shared dependencies, and parallel runtime shape.
- [[projects/telegraph/references/telegraph-entity-model]] - Communication, template, and delivery entities owned by Telegraph.
- [[projects/telegraph/references/telegraph-runtime-and-configuration]] - `TELEGRAPH_*` config loading, queue routing, template paths, and local runtime drift.
- [[projects/telegraph/references/telegraph-http-and-notification-api]] - The live notification route table, ownership checks, and OpenAPI drift.
- [[projects/telegraph/references/telegraph-event-processing]] - SQS consumption, command dispatch, descriptor loading, and delivery-mode routing.
- [[projects/telegraph/references/telegraph-testing-and-smtp-fixtures]] - Real SQS, SMTP, DB, and JWT-backed integration tests.
- [[projects/telegraph/skills/building-event-driven-notification-services]] - Reusable workflow for building Telegraph-style communication services.

## Open Questions

- The root `README.md` says Telegraph runs on port `8081` in the shared stack, while `Telegraph/docker-compose.yml` exposes `8080:8080`. Conflict to resolve. ^[ambiguous]
- The repo overview says IAMRusty publishes to `user-events`, while Telegraph's own queue-routing examples are keyed under `test-user-events`; the naming split needs a single operator-facing story. Conflict to resolve. ^[ambiguous]
- `http/src/handlers/communication.rs` defines richer send-message DTOs, but the live route table only exposes notification read-model endpoints. ^[ambiguous]

## Sources

- [[projects/rustycog/references/index]]
- [[projects/telegraph/references/telegraph-service]]
- [[projects/telegraph/references/telegraph-runtime-and-configuration]]
- [[projects/telegraph/references/telegraph-http-and-notification-api]]
- [[projects/telegraph/references/telegraph-event-processing]]
- [[projects/telegraph/references/telegraph-testing-and-smtp-fixtures]]
