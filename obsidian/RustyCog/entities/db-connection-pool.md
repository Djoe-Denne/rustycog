---
title: DbConnectionPool
category: entities
tags: [rustycog, database, runtime, visibility/internal]
sources:
  - rustycog/rustycog-db/src/lib.rs
  - rustycog/rustycog-config/src/lib.rs
summary: DbConnectionPool encapsulates write/read SeaORM connections with replica fallback and round-robin read distribution.
provenance:
  extracted: 0.91
  inferred: 0.04
  ambiguous: 0.05
created: 2026-04-15T17:15:56.0808743Z
updated: 2026-04-15T22:10:00Z
---

# DbConnectionPool

`DbConnectionPool` is the shared database connection abstraction used by RustyCog-based services.

## Key Ideas

- `DbConnectionPool` encapsulates primary-write plus optional replica-read connections.
- Read operations can route via round-robin over replicas, with fallback to primary when needed.
- It is configured from `DatabaseConfig` and usually wired once in setup/composition root.
- This entity defines the noun; connection policy details are documented in `[[projects/rustycog/references/rustycog-db]]`.

## Sources

- [[projects/rustycog/references/rustycog-db]]
- [[entities/queue-config]]
- [[concepts/structured-service-configuration]]
