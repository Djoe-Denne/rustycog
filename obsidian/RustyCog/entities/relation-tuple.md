---
title: Relation Tuple
category: entity
tags: [entity, authorization, zanzibar, openfga]
summary: >-
  Zanzibar-shaped tuple (object, relation, user) that encodes one authorization fact. Stored in OpenFGA and written exclusively by the sentinel-sync worker.
updated: 2026-04-20
---

# Relation Tuple

A relation tuple is Zanzibar's unit of authorization data: a triple of `(object, relation, user)` encoding a single fact such as "Alice owns organization 123".

## Shape

`object` and `user` are typed identifiers rendered as `"{type}:{id}"`. `relation` is one of the relations defined on `object`'s type in [openfga/model.fga](../../openfga/model.fga).

Examples:

- `organization:123#owner@user:alice`
- `project:456#organization@organization:123`
- `component:789#project@project:456`
- `notification:abc#recipient@user:bob`

## Lifecycle

- Written by [[projects/sentinel-sync/sentinel-sync]] when a domain event implies a new authorization fact.
- Deleted by `sentinel-sync` when a reverse event (`*Removed`, `*Revoked`, `*Deleted`) undoes the fact.
- Never written directly by a request-handling service.

## Read path

Clients never read tuples directly. They ask OpenFGA derived questions via [[entities/permission-checker]] (`Check`, `ListObjects`, `Expand`). OpenFGA walks the tuple graph to compute the answer.

## Related

- [[concepts/zanzibar-relation-tuples]]
- [[projects/sentinel-sync/references/event-to-tuple-mapping]]
- [[projects/sentinel-sync/references/openfga-model]]
