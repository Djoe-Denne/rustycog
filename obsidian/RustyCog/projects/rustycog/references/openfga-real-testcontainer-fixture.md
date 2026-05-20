---
title: >-
  OpenFGA Real Testcontainer Fixture
category: references
tags: [reference, rustycog, testing, openfga, visibility/internal]
sources:
  - rustycog/rustycog-testing/src/common/openfga_testcontainer.rs
  - rustycog/rustycog-config/src/lib.rs
  - rustycog/rustycog-testing/src/common/service_test_descriptor.rs
  - openfga/model.fga
  - openfga/model.json
summary: >-
  Real OpenFGA integration-test fixture that replaces the wiremock Check fake with an openfga/openfga testcontainer, fresh stores per setup, and typed tuple helpers.
provenance:
  extracted: 0.78
  inferred: 0.18
  ambiguous: 0.04
created: 2026-04-24T19:05:00Z
updated: 2026-05-20T14:06:00Z
---

# OpenFGA Real Testcontainer Fixture

`TestOpenFga` is the shared real-protocol OpenFGA fixture in [[projects/rustycog/references/rustycog-testing]]. It replaces the previous wiremock-backed `OpenFgaMockService` for Hive, Manifesto, and Telegraph integration tests.

## Runtime Model

- The fixture starts `openfga/openfga` with `GenericImage`, `run`, and `--datastore-engine=memory`.
- The singleton container is guarded by `OnceLock<Arc<Mutex<Option<Arc<TestOpenFgaContainer>>>>>`, mirroring the SQS/Kafka fixture pattern from [[skills/creating-testcontainer-fixtures]].
- Each `TestOpenFga::new()` reuses the singleton container but creates a fresh store and uploads the checked-in authorization model, so tests do not share relation tuples even when they share the container.
- The authorization model is kept as `openfga/model.fga` plus generated `openfga/model.json`; the fixture `include_str!`s the JSON body and uploads it through `POST /stores/{store_id}/authorization-models`.

## Port and Config Contract

OpenFGA test configs use the same split host/port shape as DB and SQS:

```toml
[openfga]
scheme = "http"
host = "localhost"
port = 0
store_id = ""
cache_ttl_seconds = 0
```

`OpenFgaClientConfig` lives in [[projects/rustycog/references/rustycog-config]], not `rustycog-permission`. It owns `actual_port()`, `api_url()`, and `clear_port_cache()`. The fixture loads `[openfga]` with `load_config_part::<OpenFgaClientConfig>("openfga")`, clears the OpenFGA port cache, resolves `port = 0` to a random host port, and binds container port `8080` to that resolved host port.

The fixture publishes resolved coordinates through per-service env vars:

- `MANIFESTO_OPENFGA__SCHEME`, `HOST`, `PORT`, `STORE_ID`, `AUTHORIZATION_MODEL_ID`
- `HIVE_OPENFGA__SCHEME`, `HOST`, `PORT`, `STORE_ID`, `AUTHORIZATION_MODEL_ID`
- `TELEGRAPH_OPENFGA__SCHEME`, `HOST`, `PORT`, `STORE_ID`, `AUTHORIZATION_MODEL_ID`
- `SENTINEL_SYNC_OPENFGA__SCHEME`, `HOST`, `PORT`, `STORE_ID`, `AUTHORIZATION_MODEL_ID`

This avoids fixed-port collisions and allows parallel test binaries to stand up separate OpenFGA containers. ^[inferred]

## Test API

Tests arrange real relation tuples instead of stubbing HTTP responses:

- `allow(subject, action, resource)` writes the underlying direct OpenFGA relation tuple.
- `deny(subject, action, resource)` deletes the underlying tuple.
- `allow_wildcard` / `deny_wildcard` handle `user:*` public-read arrangements.
- `allow_all` grants every standard permission for broad happy-path setup.
- `write_tuple` / `delete_tuple` are escape hatches for structural tuples that are not represented by `Permission`.
- `read_tuples` exposes OpenFGA's read endpoint for assertions.
- `reset()` recreates the store and re-uploads the model, but must be called before the service boots because already-built checkers hold the previous `store_id`.

`Permission::relation()` returns derived check relations (`read`, `write`, `administer`, `own`), but OpenFGA only accepts writes to direct relations. The fixture's `writable_relation_for(object_type, action)` translates test permissions into writable relations such as `viewer`, `member`, `admin`, `owner`, `editor`, and `recipient`.

## Test Migration Implications

The old wiremock fake usually started from a permissive default (`mock_check_any(true)`) and then overrode specific denial cases. The real OpenFGA fixture denies by default, so every happy-path protected route must seed explicit tuples before the request. Denial tests usually need no tuple setup.

For create flows where production authorization tuples would normally be written asynchronously by [[projects/sentinel-sync/references/sentinel-sync-worker]], the test must either seed the expected tuple manually or explicitly simulate the sync result after the domain event is created. ^[inferred]

## Related

- [[projects/rustycog/references/rustycog-testing]]
- [[projects/rustycog/references/rustycog-config]]
- [[projects/rustycog/references/rustycog-permission]]
- [[concepts/integration-testing-with-real-infrastructure]]
- [[skills/creating-testcontainer-fixtures]]
- [[projects/sentinel-sync/references/sentinel-sync-worker]]
