---
title: Extending Sentinel-Sync With New Events
category: skill
tags: [skill, sentinel-sync, authorization, openfga]
summary: Step-by-step for adding a new domain event -> OpenFGA tuple mapping without breaking idempotency or the engine-neutral contract.
updated: 2026-04-20
---

# Extending Sentinel-Sync With New Events

Use this skill when adding authorization relevance to a new domain event, or when a new producer service starts publishing.

## 1. Confirm the event shape

Find the event variant in the producer's `*-events` crate (`hive-events`, `manifesto-events`, `iam-events`, or a new `*-events` crate). Note:

- The exact `#[serde(rename = "...")]` tag so the raw JSON matches the `event_type` field.
- Every UUID field the translator will read.

## 2. Design the tuple delta

Choose:

- The OpenFGA type (must already exist in [openfga/model.fga](../../../../openfga/model.fga)).
- The relation name (`owner`, `admin`, `member`, `viewer`, `editor`, `recipient`, or a parent relation like `organization`).
- The subject (usually `user:{uuid}` or a parent object like `project:{uuid}`).

Write the change table row in [[projects/sentinel-sync/references/event-to-tuple-mapping]] before coding.

## 3. Extend the translator

Edit `sentinel-sync/src/translator/{service}.rs` and add the match arm. Example:

```rust
ManifestoDomainEvent::ComponentAdded(evt) => Ok(Some(
    TupleDelta::default()
        .write(Tuple::object(
            "component",
            evt.component_id,
            "project",
            "project",
            evt.project_id,
        )),
)),
```

Use `Tuple::user(...)` for `user:{uuid}` subjects and `Tuple::object(...)` for parent-relation subjects.

## 4. Keep mappings small and explicit

- Prefer one translator match arm per event variant. Avoid shared helpers that hide which event produces which tuple.
- Never read domain state during translation. The event payload must carry every ID the translator needs.
- Empty deltas are fine for events with no authz relevance.

## 5. Handle reversal events

For every `*Created` / `*Added` / `*Granted`, pair with the reverse event (`*Deleted` / `*Removed` / `*Revoked`). The reverse arm emits `TupleDelta::default().delete(...)` for the same tuple.

## 6. Extend the OpenFGA model if needed

If the event requires a new type or relation, edit [openfga/model.fga](../../../../openfga/model.fga) first and capture the change in [[projects/sentinel-sync/references/openfga-model]] before writing the translator arm. The model must be uploaded (Playground or `fga` CLI) before the new tuples can be written.

## 7. Idempotency rules

- Do not short-circuit the ledger in the translator. `SyncEventHandler` already guarantees each `event_id` is processed exactly once.
- Translators must be pure functions of the event payload — any external lookup breaks retry safety.

## 8. Test

- Add a unit test on the translator with a hand-built event payload and an expected `TupleDelta`.
- Run `cargo test -p sentinel-sync`.
- Optionally run `docker compose up -d postgres create-databases openfga-migrate openfga` (from the root) and exercise the happy path end-to-end against `http://localhost:8090`.
