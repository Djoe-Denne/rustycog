---
title: Using RustyCog DB
category: skills
tags: [rustycog, database, skills, visibility/internal]
sources:
  - rustycog/rustycog-db/src/lib.rs
  - rustycog/rustycog-config/src/lib.rs
summary: Practical setup pattern for rustycog-db DbConnectionPool with write/read routing and replica fallback behavior.
provenance:
  extracted: 0.9
  inferred: 0.04
  ambiguous: 0.06
created: 2026-04-15T17:15:56.0808743Z
updated: 2026-04-15T17:15:56.0808743Z
---

# Using RustyCog DB

Use this guide when integrating `<!-- [[projects/rustycog/references/rustycog-db]] -->` into service setup.

## Workflow

- Define `DatabaseConfig` in your service config and load it before building repositories.
- Create one shared `DbConnectionPool` with `DbConnectionPool::new(&db_config)`.
- Pass `get_write_connection()` into write repositories and `get_read_connection()` into read/query repositories.
- Configure read replicas only when needed; fallback to primary is automatic when none are available.
- Keep repository wiring in setup/composition root so business logic remains storage-agnostic.

## Common Pitfalls

- Sending all read workloads to `get_write_connection()` and bypassing replica routing.
- Ignoring replica connection failures in startup logs.
- Recreating pools in many handlers instead of sharing one pool instance.

## Sources

- <!-- [[projects/rustycog/references/rustycog-db]] -->
- <!-- [[entities/db-connection-pool]] -->
- <!-- [[concepts/structured-service-configuration]] -->
