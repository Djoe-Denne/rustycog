---
title: >-
  Running AIForAll Runtime Modes
category: skills
tags: [platform, operations, rust, architecture, visibility/internal]
sources:
  - README.md
  - Cargo.toml
  - monolith/Cargo.toml
  - monolith/src/routes.rs
  - monolith/src/runtime.rs
  - IAMRusty/http/src/lib.rs
  - Telegraph/http/src/lib.rs
  - Hive/http/src/lib.rs
  - Manifesto/http/src/lib.rs
summary: >-
  Operational workflow for choosing, compiling, and smoke-testing AIForAll as standalone microservices or as the oodhive-monolith modular monolith.
provenance:
  extracted: 0.83
  inferred: 0.14
  ambiguous: 0.03
created: 2026-04-25T10:10:00Z
updated: 2026-04-25T10:10:00Z
---

# Running AIForAll Runtime Modes

Use this workflow when you need to validate whether AIForAll is being run as independent Rust services or as the `oodhive-monolith` modular monolith.

## Choose The Runtime

- Use **microservice mode** when you want each service package to bind its own listener: `iam-service`, `telegraph-service`, `hive-service`, and `manifesto-service`.
- Use **monolith mode** when you want one process and one HTTP listener from the `oodhive-monolith` package.
- Do not change route URLs between modes. Both modes use bounded-context prefixes: `/iam`, `/telegraph`, `/hive`, and `/manifesto`.

## Compile Checks

For standalone service mode:

```bash
cargo check -p iam-service
cargo check -p telegraph-service
cargo check -p hive-service
cargo check -p manifesto-service
```

For monolith mode:

```bash
cargo check -p oodhive-monolith
```

## Smoke-Test Paths

The route prefix is part of the contract. Health checks should use the same shape regardless of runtime mode:

```bash
curl http://localhost:8080/iam/health
curl http://localhost:8080/telegraph/health
curl http://localhost:8080/hive/health
curl http://localhost:8080/manifesto/health
```

In monolith mode, there is also a top-level process health route:

```bash
curl http://localhost:8080/health
```

Representative nested API checks:

```bash
curl http://localhost:8080/iam/.well-known/jwks.json
curl http://localhost:8080/hive/api/organizations/search
curl http://localhost:8080/manifesto/api/projects
curl http://localhost:8080/telegraph/api/notifications
```

Authenticated routes may return `401` or `403`; for smoke testing, the important signal is that the request resolves through the expected bounded-context router.

## Guardrails

- Do not make `oodhive-monolith` call service `run()` methods. It should build each service via setup, extract routers, start only Telegraph and Manifesto background tasks, then serve one composed Axum router.
- Keep SQS/event infrastructure unchanged unless explicitly planning a new event-bus design.
- Keep standalone service prefixes aligned with `monolith/src/routes.rs` and the `SERVICE_PREFIX` constants in each HTTP crate.

## Related

- [[projects/aiforall/references/modular-monolith-runtime]]
- [[projects/rustycog/references/rustycog-http]]
- [[concepts/event-driven-microservice-platform]]
- [[concepts/shared-rust-microservice-sdk]]
