---
title: Zanzibar Relation Tuples
category: concept
tags: [concept, authorization, zanzibar, openfga]
summary: >-
  Zanzibar models authorization as (object, relation, user) tuples forming a relation graph. AIForAll stores these tuples in OpenFGA and derives every access decision by traversing the graph, rather than writing matchers per resource type.
updated: 2026-04-20
---

# Zanzibar Relation Tuples

Google's Zanzibar paper introduced the idea of expressing authorization as a graph of relation tuples instead of as per-resource matchers. AIForAll follows that model through OpenFGA.

## Tuple shape

Each tuple is `(object, relation, user)`:

- `organization:123#owner@user:alice` — Alice owns organization 123.
- `project:456#organization@organization:123` — project 456 belongs to organization 123.
- `component:789#project@project:456` — component 789 belongs to project 456.

The relation graph answers questions like "can Alice write component 789?" by walking edges until it either finds an allow-edge or exhausts the search.

## Derived relations

The OpenFGA model at [openfga/model.fga](../../openfga/model.fga) derives higher-level relations from lower-level ones. Examples:

- `admin: [user] or owner` — owners are automatically admins.
- `viewer: [user] or member or viewer from organization` — org viewers see every project.
- `component#editor: [user] or admin from project` — project admins edit every component.

This replaces Casbin matchers like `r.sub == p.sub && r.project == p.project && r.act == p.act` with declarative inheritance.

## Writing tuples

The [[projects/sentinel-sync/sentinel-sync]] worker is the sole writer. It listens to domain events and maps each one to `Write` or `Delete` calls (see [[projects/sentinel-sync/references/event-to-tuple-mapping]]).

## Reading tuples

Client services never read tuples directly. They call `Check` (or `ListObjects` / `Expand` when needed) through [[entities/permission-checker]]. OpenFGA returns a boolean decision.

## Related

- [[concepts/openfga-as-authorization-engine]]
- [[entities/relation-tuple]]
