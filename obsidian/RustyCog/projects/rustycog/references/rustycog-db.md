---
title: RustyCog DB
category: references
tags: [reference, rustycog, database, visibility/internal]
sources:
  - rustycog/rustycog-db/src/lib.rs
  - rustycog/rustycog-config/src/lib.rs
summary: rustycog::db implements DbConnectionPool with write/read split, replica fallback, and round-robin read routing over SeaORM connections.
provenance:
  extracted: 0.9
  inferred: 0.04
  ambiguous: 0.06
created: 2026-04-15T17:15:56.0808743Z
updated: 2026-05-20T14:04:00Z
---

# RustyCog DB

`rustycog::db` (historically `rustycog-db`) wraps SeaORM connection management behind a shared `DbConnectionPool` used by services built on `[[projects/rustycog/rustycog]]`.

## Key Ideas

- `DbConnectionPool::new()` takes `DatabaseConfig`, opens a primary write connection, and then attempts to connect every configured read replica.
- If no read replicas are configured, or if all replicas fail, reads fall back to the write connection.
- `get_read_connection()` uses round-robin selection when multiple read connections are available.
- `new_from_url()` keeps backward compatibility for services that still pass one URL string plus replica URLs.
- The module keeps pool setup policy (timeouts, connection counts, logging) centralized so services do not re-implement boilerplate.

## Linked Entities

- [[entities/db-connection-pool]]
- [[entities/queue-config]]

## Open Questions

- Pool tuning values are hardcoded in the crate, so service-specific tuning policies still require either wrapper code or crate changes. ^[inferred]

## Sources

- [[projects/rustycog/references/index]]
- [[concepts/structured-service-configuration]]
- [[projects/rustycog/rustycog]]
