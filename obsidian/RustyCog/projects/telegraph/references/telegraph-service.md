---
title: Telegraph Service
category: references
tags: [reference, communication, architecture, visibility/internal]
sources:
  - README.md
  - Telegraph/setup/src/app.rs
  - Telegraph/infra/src/event/consumer.rs
  - Telegraph/http/src/lib.rs
  - Telegraph/configuration/src/lib.rs
summary: Telegraph-specific runtime notes that sit on top of RustyCog's shared service shell, emphasizing its parallel queue plus HTTP design and notification-focused boundaries.
provenance:
  extracted: 0.81
  inferred: 0.12
  ambiguous: 0.07
created: 2026-04-14T18:18:24.0602572Z
updated: 2026-04-19T12:08:26.9393504Z
---

# Telegraph Service

This page is the Telegraph-specific companion to `[[projects/rustycog/references/index]]` and `[[references/rustycog-service-construction]]`. It skips the generic RustyCog service shell and keeps the parts of `[[projects/telegraph/telegraph]]` that are meaningfully different.

## RustyCog Baseline

- `[[projects/rustycog/references/index]]` is the shared reference map for the service layout, command runtime, config loading, HTTP shell, queue transport, and testing harness this page assumes.
- `[[references/rustycog-service-construction]]` and `[[skills/building-rustycog-services]]` cover the generic composition-root story that Telegraph reuses.
- `[[projects/rustycog/references/rustycog-command]]`, `[[projects/rustycog/references/rustycog-config]]`, `[[projects/rustycog/references/rustycog-events]]`, and `[[projects/rustycog/references/rustycog-http]]` explain the shared primitives that are not repeated here.

## Service-Specific Differences

- Telegraph keeps the same broad crate split used elsewhere in the repo, but its composition root is specialized around email, templates, notifications, and event-consumer wiring rather than CRUD-style HTTP only.
- `setup/src/app.rs` creates the email adapter and service, template service, database pool, repositories, notification service, permission fetcher, communication factory, event processor, command registry, command service, and final event consumer.
- `TelegraphApp::run()` starts the event consumer and the HTTP server in parallel and waits on both with `tokio::select!`, so queue processing is a first-class runtime path rather than an auxiliary background worker.
- HTTP exposure is intentionally narrow: the live server only wires notification read-model routes, while richer communication DTOs remain present in code but not in the active route table. ^[ambiguous]
- The root repo README describes Telegraph as the platform communication service for emails, notifications, and SMS, but the service-level code currently resolves that promise mainly through email plus notification flows. ^[ambiguous]

## Open Questions

- Telegraph has no project-local README in this source tree, so the top-level repo README and the code are doing most of the documentation work today.
- The queue consumer is arguably the service's primary ingress path, but the current codebase does not declare one canonical “main interface” between HTTP and queue processing. ^[inferred]

## Sources

- [[projects/telegraph/telegraph]] - Main project overview.
- [[projects/telegraph/concepts/queue-driven-command-processing]] - How async events are pushed through the command runtime.
- [[projects/telegraph/references/telegraph-runtime-and-configuration]] - Runtime config, queue routing, templates, and local deployment details.
- [[projects/telegraph/references/telegraph-http-and-notification-api]] - Authenticated route surface and ownership model.
- [[projects/rustycog/references/index]] - Crate-level map for the shared SDK surfaces Telegraph composes.
