---
title: RustyCog References Index
category: navigation
tags: [index, references, sdk]
summary: >-
  Canonical map from the `rustycog-framework` package's feature-gated modules to detailed references plus shared concept/entity/skill pages.
provenance:
  extracted: 0.9
  inferred: 0.06
  ambiguous: 0.04
updated: 2026-05-20T13:58:00Z
---

# RustyCog References

This index is the canonical semantic map for RustyCog's unified package layout: `rustycog-framework` is normally aliased as `rustycog`, and its features expose the modules below.

## Inventory

- [[projects/rustycog/references/rustycog-core]]
- [[projects/rustycog/references/rustycog-command]]
- [[projects/rustycog/references/rustycog-config]]
- [[projects/rustycog/references/rustycog-db]]
- [[projects/rustycog/references/rustycog-events]]
- [[projects/rustycog/references/rustycog-outbox]]
- [[projects/rustycog/references/rustycog-http]]
- [[projects/rustycog/references/rustycog-permission]]
- [[projects/rustycog/references/rustycog-testing]]
- [[projects/rustycog/references/openfga-real-testcontainer-fixture]]
- [[projects/rustycog/references/wiremock-mock-server-fixture]]
- [[projects/rustycog/references/openfga-mock-service]]
- [[projects/rustycog/references/rustycog-server]]
- [[projects/rustycog/references/rustycog-logger]]
- [[projects/rustycog/references/rustycog-meta]] (legacy note)

## Workspace and Packaging Reality

- The framework layer now has one runtime package, `rustycog-framework`, plus the separate `rustycog-testing` package for integration fixtures.
- Consumers normally declare `rustycog = { package = "rustycog-framework", ... }`, then import `rustycog::core`, `rustycog::http`, and the other modules listed below.
- `Cargo.toml` defines one package with feature flags (`core`, `command`, `config`, `db`, `events`, `outbox`, `http`, `permission`, `server`, `logger`, `test-utils`, `full`).
- `rustycog/src/lib.rs` maps these features to the historical source folders (`rustycog-*/src`) via `#[path]`, so source layout stays stable.
- `rustycog-testing` remains separate and depends on `rustycog` with `full` + `test-utils`.

## Feature/Module Semantic Map

- `rustycog::core` (`core`) -> entities: [[entities/service-error]], [[entities/domain-error]] | concept: [[concepts/shared-rust-microservice-sdk]] | skill: [[skills/using-rustycog-core]]
- `rustycog::command` (`command`) -> entities: [[entities/command-registry]], [[entities/command-context]] | concept: [[concepts/command-registry-and-retry-policies]] | skill: [[skills/using-rustycog-command]]
- `rustycog::config` (`config`) -> entities: [[entities/queue-config]] | concept: [[concepts/structured-service-configuration]] | skill: [[skills/using-rustycog-config]] | OpenFGA config: [[projects/rustycog/references/openfga-real-testcontainer-fixture]]
- `rustycog::db` (`db`) -> entities: [[entities/db-connection-pool]] | concept: [[concepts/structured-service-configuration]] | skill: [[skills/using-rustycog-db]]
- `rustycog::events` (`events`) -> entities: [[entities/domain-event]], [[entities/event-publisher]], [[entities/queue-config]] | concept: [[concepts/event-driven-microservice-platform]] | skill: [[skills/using-rustycog-events]] | durability bridge: [[projects/rustycog/references/rustycog-outbox]]
- `rustycog::outbox` (`outbox`) -> entities: [[entities/domain-event]], [[entities/event-publisher]], [[entities/db-connection-pool]] | concepts: [[concepts/event-driven-microservice-platform]]
- `rustycog::http` (`http`) -> entities: [[entities/route-builder]], [[entities/resource-id]], [[entities/permission-checker]], [[entities/resource-ref]] | concept: [[concepts/centralized-authorization-service]] | skill: [[skills/using-rustycog-http]]
- `rustycog::permission` (`permission`) -> entities: [[entities/permission-checker]], [[entities/subject]], [[entities/resource-ref]], [[entities/resource-id]] | concept: [[concepts/openfga-as-authorization-engine]] | skill: [[skills/using-rustycog-permission]]
- `rustycog-testing` crate -> entities: [[entities/event-publisher]], [[entities/queue-config]], [[entities/route-builder]] | concept: [[concepts/integration-testing-with-real-infrastructure]] | skills: [[skills/using-rustycog-testing]], [[skills/stubbing-http-with-wiremock]], [[skills/creating-testcontainer-fixtures]] | fixtures: [[projects/rustycog/references/wiremock-mock-server-fixture]], [[projects/rustycog/references/openfga-real-testcontainer-fixture]], [[projects/rustycog/references/openfga-mock-service]]
- `rustycog::logger` (`logger`) -> concept: [[concepts/structured-service-configuration]] | skill: [[skills/using-rustycog-logger]]
- `rustycog::server` (`server`) -> entity: [[entities/health-checker]] | usage guidance is documented directly in [[projects/rustycog/references/rustycog-server]]
- `rustycog-meta` -> legacy packaging note only; see [[projects/rustycog/references/rustycog-meta]]

## Build Workflow

- End-to-end composition playbook: [[skills/building-rustycog-services]]
- Guide-vs-runtime drift analysis: [[references/rustycog-service-construction]]

## Known Gaps To Track

- `rustycog-server` naming still suggests broader server bootstrap scope than the current health-only surface. ^[ambiguous]
- `create_multi_queue_event_publisher()` accepts queue sets but currently wraps one publisher instance. ^[ambiguous]
- `rustycog-config` SQS endpoint conventions still mix AWS and Scaleway vocabulary. ^[ambiguous]
