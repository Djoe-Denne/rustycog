---
title: >-
  Creating Testcontainer Fixtures
category: skills
tags: [skills, testing, testcontainers, fixtures, docker, visibility/internal]
sources:
  - rustycog/rustycog-testing/src/common/sqs_testcontainer.rs
  - rustycog/rustycog-testing/src/common/kafka_testcontainer.rs
  - rustycog/rustycog-testing/src/common/service_test_descriptor.rs
  - rustycog/rustycog-testing/src/common/openfga_testcontainer.rs
  - rustycog/rustycog-config/src/lib.rs
  - Hive/tests/sqs_event_routing_tests.rs
  - IAMRusty/tests/sqs_event_routing_tests.rs
  - Manifesto/tests/sqs_event_routing_tests.rs
  - Telegraph/tests/fixtures/smtp/testcontainer.rs
  - Telegraph/config/test.toml
  - IAMRusty/config/test.toml
  - Manifesto/config/test.toml
summary: >-
  Recipe for adding a real Docker-backed testcontainer fixture, including shared vs service-local placement, stale-container cleanup, and port = 0 config wiring.
provenance:
  extracted: 0.74
  inferred: 0.22
  ambiguous: 0.04
created: 2026-04-23T19:30:00Z
updated: 2026-04-25T11:25:00Z
---

# Creating Testcontainer Fixtures

Use this recipe when an integration test needs to assert against a **real** protocol — wire-level SMTP, real SQS message dispatch, Kafka ack semantics, Postgres SQL behavior — instead of stubbing the collaborator with [[skills/stubbing-http-with-wiremock|wiremock]]. The shared crate already provides reusable Postgres, LocalStack-SQS, and Kafka fixtures; this page is for adding the *next* one (Redis, Mongo, Vault, Localstack-S3, MinIO, NATS, etc.).

For SQS producer-routing tests, use the fixture as a queue spy, not just a broker bootstrapper: configure all queue names through `SqsConfig`, keep shared `test.toml` queue settings `enabled = false`, then opt in from the routing test binary with `has_sqs() == true` plus a service env override such as `HIVE_QUEUE__ENABLED=true`. Drain every queue involved in the assertion, wait on the mapped queue by name, and verify the fallback queue stayed empty. Current worked examples are `Hive/tests/sqs_event_routing_tests.rs`, `IAMRusty/tests/sqs_event_routing_tests.rs`, and `Manifesto/tests/sqs_event_routing_tests.rs`.

## Step 0: Pick where the fixture lives

| Where | When | Wiki examples |
|---|---|---|
| `rustycog/rustycog-testing/src/common/<thing>_testcontainer.rs` | The infra is a generic platform capability multiple services will reuse | `sqs_testcontainer.rs`, `kafka_testcontainer.rs`, `openfga_testcontainer.rs` ([[projects/rustycog/references/rustycog-testing]]) |
| `<Service>/tests/fixtures/<thing>/testcontainer.rs` | Only that service interacts with this protocol, or wire-level parsing is service-specific | Telegraph's `TestSmtp` MailHog container ([[projects/telegraph/references/telegraph-testing-and-smtp-fixtures]]) |

The heuristic: if production has a `[[projects/rustycog/rustycog]]` shared client for it (event publisher, DB pool), the fixture goes shared. If only one service speaks to it, keep it service-local — the rest of the workspace doesn't need a Cargo dependency on a container they will never start.

## Step 1: Extend the descriptor (shared fixtures only)

`ServiceTestDescriptor<T>` in `rustycog-testing` is the central capability contract; the trait is **not** defaulted, so adding a flag is a breaking change every implementor must absorb:

```rust
pub trait ServiceTestDescriptor<T>: Send + Sync + 'static {
    type Config: ...;
    async fn build_app(&self, ...) -> anyhow::Result<()>;
    async fn run_app(&self, ...) -> anyhow::Result<()>;
    async fn run_migrations_up(&self, ...) -> anyhow::Result<()>;
    async fn run_migrations_down(&self, ...) -> anyhow::Result<()>;
    fn has_db(&self) -> bool;
    fn has_sqs(&self) -> bool;
    // Add for a new shared fixture:
    fn has_redis(&self) -> bool;
}
```

Each per-service descriptor (`HiveTestDescriptor`, `TelegraphTestDescriptor`, `ManifestoTestDescriptor`, `IamServiceTestDescriptor`) opts in by overriding the new method. The shared fixture builder branches on the flag so services that don't need the container pay zero startup cost.

OpenFGA is the current non-queue example: `ServiceTestDescriptor` has `has_openfga()`, and only Hive, Manifesto, and Telegraph opt in to the real [[projects/rustycog/references/openfga-real-testcontainer-fixture]].

Skip this step entirely if you're going service-local — there's nothing to extend, you just construct the fixture directly inside that service's `tests/common.rs`.

## Step 2: Build the testcontainer wrapper

Both shared and service-local fixtures follow the same scaffold (`sqs_testcontainer.rs`, `kafka_testcontainer.rs`, `Telegraph/tests/fixtures/smtp/testcontainer.rs`). The five non-negotiable pieces:

### 2a. Process-wide singleton

```rust
static TEST_<THING>_CONTAINER: OnceLock<Arc<Mutex<Option<Arc<Test<Thing>Container>>>>> = OnceLock::new();
```

The triple-wrap is not paranoia — each layer earns its keep:

- `OnceLock` lazily constructs the slot the first time any test asks for it.
- `Arc<Mutex<...>>` lets the slot be shared across the whole test process and serializes container start/stop.
- `Option<Arc<...>>` distinguishes "container not yet started" from "container running, here's the handle".
- The inner `Arc<Test<Thing>Container>` lets multiple test fixtures hold cheap clones without fighting for ownership.

### 2b. Container wrapper struct

```rust
pub struct Test<Thing>Container {
    container: ContainerAsync<GenericImage>,
    pub endpoint_url: String,
    pub port: u16,
}

impl Test<Thing>Container {
    pub async fn cleanup(self) {
        if let Err(e) = self.container.stop().await { warn!("..."); }
        if let Err(e) = self.container.rm().await { warn!("..."); }
    }
}
```

Owning the `ContainerAsync` directly means `testcontainers`' own `Drop` impl tears the container down when the singleton is dropped. The explicit `cleanup()` is for the manual path the harness exposes for orderly shutdown.

### 2c. `get_or_create_<thing>_container()`

This is the heart of the fixture. The pattern from `sqs_testcontainer.rs`:

1. Acquire the singleton mutex.
2. If a container is already running, return it (plus the resolved config).
3. Otherwise, **call `cleanup_existing_<thing>_container().await` first** to evict any stale container left from a previous interrupted run.
4. Resolve a port (see Step 3).
5. Build the `GenericImage` with `with_container_name("<service>_test-<thing>")`, env vars, and `with_mapped_port(...)` to pin the host side.
6. `start().await` the container.
7. Stash it in the singleton slot, return.

### 2d. Defensive Docker-level cleanup

```rust
async fn cleanup_existing_<thing>_container() {
    use std::process::Command;
    let containers = ["<service>_test-<thing>"];
    for container_name in &containers {
        let _ = Command::new("docker").args(&["stop", container_name]).output();
        let _ = Command::new("docker").args(&["rm", "-f", container_name]).output();
    }
}
```

This is the safety net for `Ctrl-C` and panics in startup — the next `cargo test` would otherwise fail to bind because the previous container is still grabbing the port. The container name **must** be unique per fixture; SQS uses `iam_test-localstack-sqs`, MailHog uses `telegraph_test-smtp`. Reusing a name across fixtures is the easiest way to cause confusing cross-suite failures.

### 2e. Typed client API

Whatever the production code uses to talk to this infra, expose the same shape from the fixture. The wiki's two reference shapes:

- **SQS** wraps the official AWS SDK client and re-exports message helpers (`receive_messages`, `wait_for_messages`, `purge_queue`, `verify_event_published`).
- **MailHog** doesn't have a client SDK, so `TestSmtp` wraps `reqwest::Client` and calls MailHog's `/api/v1/messages` REST API directly, returning typed `TestEmail` values (`get_emails`, `email_count`, `has_email`, `clear_emails`).

The principle: **tests should never speak the raw protocol**. If a test ends up calling `client.send().queue_url(...).message_body(...)` directly, that's a missing helper on the fixture.

## Step 3: Wire the container's port into `test.toml`

Two patterns the wiki documents, with different trade-offs:

### `port = 0` + env-var publication

This is what SQS, DB, and OpenFGA do. `test.toml` declares `port = 0`, the fixture asks the config layer for an `actual_port()` (which caches a random free port), then **mutates env vars** so the rest of the app picks up the resolved values:

```rust
// from sqs_testcontainer.rs
unsafe {
    std::env::set_var("IAM_QUEUE__TYPE", "sqs");
    std::env::set_var("IAM_QUEUE__SQS__HOST", host);
    std::env::set_var("IAM_QUEUE__SQS__PORT", &port.to_string());
    // ...
}
```

The `unsafe` block is unavoidable on modern Rust because `set_var` is `unsafe` since `std::env` was tightened. Confining the env-mutation to the testcontainer constructor keeps the surface small.

If the service config currently exposes only a single URL string such as `api_url`, split it before adding the fixture. The OpenFGA migration moved `OpenFgaClientConfig` into [[projects/rustycog/references/rustycog-config]] and changed it to `scheme` / `host` / `port` so `[openfga] port = 0` can resolve exactly like `[database]` and `[queue]`. A fixture that grabs a random port with `TcpListener::bind("127.0.0.1:0")` but leaves typed config as a flat URL has no clean way for the app boot path to resolve the same port. ^[inferred]

### Fixed mapped port

This is what MailHog does. `Telegraph/config/test.toml` pins `smtp.port = 1025`, and the container does:

```rust
.with_mapped_port(smtp_config.port, ContainerPort::Tcp(1025))
.with_mapped_port(8025, ContainerPort::Tcp(8025))  // MailHog admin API
```

Easier when the protocol has a well-known port and an admin API URL needs to stay stable across restarts. The cost: **parallel CI jobs sharing a host will collide.** Only safe when the runner reliably gives each job its own kernel namespace.

The default choice should be `port = 0` for new fixtures. Switch to fixed only when the protocol or the test harness can't tolerate a moving port.

## Step 4: Wire the fixture into `setup_test_server()`

Mirror what Telegraph and the existing SQS path do: construct the container *before* the app boots, then clear any prior in-container state at the top of `setup_test_server()` for isolation. From [[projects/telegraph/references/telegraph-testing-and-smtp-fixtures]]:

> `setup_test_server()` creates a `TelegraphTestFixture`, clears prior SMTP state, then boots the app through the shared RustyCog test-server path so the service is exercised with real infrastructure rather than a mocked shell.

If your fixture supports a `clear_<state>()` (purge messages, drop a Redis DB, truncate buckets), call it from `setup_test_server()` so each test starts on a known floor. For the actual app handle, expand the tuple `setup_test_server` returns the way Manifesto and Hive do for the OpenFGA mock — see [[projects/rustycog/references/openfga-mock-service]] for the canonical "factory returns a service-handle alongside the boot bundle" shape.

## Step 5: Wait for ready

Containers report "started" before they accept connections. Skipping the readiness probe gives the first test a flaky timeout. The minimum:

```rust
async fn wait_for_ready(&self) -> Result<(), Box<dyn std::error::Error>> {
    let url = format!("http://{}:{}/<healthz>", self.host, self.port);
    for _ in 0..30 {
        match self.client.get(&url).send().await {
            Ok(r) if r.status().is_success() => return Ok(()),
            _ => tokio::time::sleep(Duration::from_millis(100)).await,
        }
    }
    Err("readiness timeout".into())
}
```

LocalStack ships an `/_localstack/health` endpoint, MailHog uses `/api/v1/messages`, Postgres needs a `SELECT 1`. Pick one that succeeds *only* once the protocol you actually use is up — TCP-port-open is not the same as "Kafka has elected a broker leader".

## Common Pitfalls

- **Reusing a container name across fixtures.** The defensive Docker cleanup in step 2d uses an exact-match name list; a duplicated name means stopping and removing a sibling fixture's container by mistake. Name uniquely (`<service>_test-<thing>`).
- **Forgetting to call `cleanup_existing_<thing>_container().await` before starting.** A `Ctrl-C` between test runs leaves the old container holding the port — the next run fails with a confusing "address already in use" instead of cleanly evicting the stale one.
- **Holding only a clone of the inner client without the singleton.** If the only `Arc<Test<Thing>Container>` reference goes out of scope during teardown, the container drops and the next test has to start a fresh one. Keep the `OnceLock` slot populated for the whole test process.
- **Leaving `port = 0` *and* hard-coding the port elsewhere.** The whole point of `port = 0` is the fixture publishes the resolved port via env. If a different config file or a `const` still has `1025` baked in, the service-under-test connects to the wrong place and the test sees no traffic.
- **Keeping a single `api_url` field for a random-port fixture.** Random host ports need a typed `port` slot plus `actual_port()` cache. Use `api_url()` as a computed method, not as the stored config field.
- **Polling without a deadline.** Both `wait_for_messages` and `wait_for_ready` cap their loops with `max_wait_secs` / a fixed iteration count. A bare `while !ready { sleep(100ms).await }` will hang the suite when the container fails to start, with no useful error.
- **Skipping `#[serial]`.** All the shared-singleton fixtures rely on the singleton not being raced by parallel tests. The same `serial_test::serial` rule that applies to the wiremock fixture applies here. ^[inferred]
- **Calling `set_var` outside the fixture constructor.** Env-var mutation is process-global; if multiple tests set conflicting values the last writer wins. Confine env mutation to `get_or_create_<thing>_container()` and never touch the same vars from a test body.
- **Letting the production cache mask the round-trip.** Same as for wiremock — a `CachedXxxClient` decorator that the production wiring adds will swallow the second request. Make the cache TTL configurable (the [[projects/rustycog/references/rustycog-permission|`cache_ttl_seconds`]] pattern) and disable it in tests.

## When Not To Use This

If the test only needs to assert on what the service *would have sent* under a controlled response, [[skills/stubbing-http-with-wiremock|wiremock]] is faster, more deterministic, and parallelizes better. Real testcontainers earn their cost when the assertion targets the protocol itself: SMTP framing, queue-broker semantics, SQL parser behavior, real serializer output, etc. Telegraph deliberately keeps both `SmtpService` (wiremock) and `TestSmtp` (MailHog testcontainer) side by side for exactly this reason — see [[projects/telegraph/references/telegraph-testing-and-smtp-fixtures]].

## Sources

- [[projects/rustycog/references/rustycog-testing]] — descriptor model and the catalog of existing shared fixtures.
- [[concepts/integration-testing-with-real-infrastructure]] — when to choose real infra over mocks.
- [[projects/telegraph/references/telegraph-testing-and-smtp-fixtures]] — canonical service-local testcontainer with both REST-API and protocol surfaces.
- [[projects/rustycog/references/wiremock-mock-server-fixture]] — sister pattern for the HTTP-stub case; cross-reference when both fixtures coexist.
- [[skills/stubbing-http-with-wiremock]] — the stub-based alternative this skill complements.
- [[projects/rustycog/references/openfga-mock-service]] — factory-returning-handle example to mirror when extending `setup_test_server()`.
- [[projects/rustycog/references/openfga-real-testcontainer-fixture]] — shared real OpenFGA fixture and the host/port config migration example.
