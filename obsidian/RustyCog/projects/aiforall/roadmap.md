---
title: AIForAll Roadmap
category: roadmap
tags: [platform, testing, database, events, visibility/internal]
summary: >-
  Near-term roadmap for AIForAll: Sentinel Sync service tests, transaction-ready DB workflows, RustyCog Events outbox, and IAM external-provider extraction.
status: planned
created: 2026-04-25T11:42:00Z
updated: 2026-04-26T13:36:00Z
---

# AIForAll Roadmap

This roadmap captures the next platform functionality focus. The immediate direction is to turn the current service and event plumbing into verified behavior: authorization sync must be tested, database transaction behavior must be measured, event publication must survive domain-write failure modes, and IAM provider integrations should be extensible without changing IAM itself.

## Near-Term Focus

### Sentinel-5 / Sentinel Sync service tests

The first focus area is the test strategy for [[projects/sentinel-sync/sentinel-sync|Sentinel Sync]], especially the worker path that consumes service events and writes OpenFGA relation tuples.

Focus:

- Cover the `sentinel-sync` translator and handler boundaries with realistic service events.
- Verify the event-to-tuple mapping documented in [[projects/sentinel-sync/references/event-to-tuple-mapping]].
- Exercise idempotency so repeated `event_id` values do not duplicate relation writes.
- Prefer real protocol fixtures where the failure mode matters, especially queue delivery and OpenFGA writes.

Done means the service has confidence-building tests around the [[projects/sentinel-sync/references/sentinel-sync-worker]] path: valid events create or delete the expected tuples, unsupported or malformed events fail predictably, and repeated delivery is safe.

### Transaction-ready DB workflows

The second focus area is proving how the database model behaves under transactional load. This is about validating the practical limits and guarantees of the current persistence design rather than assuming that pool configuration and schema shape are sufficient.

Progress:

- RustyCog DB now exposes a primary/write transaction entry point for workflows that must avoid read-replica lag.
- Manifesto project creation now has a transaction-backed unit of work covering the project row, owner member, and owner role grants.
- IAMRusty now treats signup user/email/verification creation and refresh-token rotation as atomic persistence flows.
- Telegraph now creates notification records and delivery records in one transaction.
- Sentinel Sync idempotency now distinguishes `begin`, `complete`, and `fail`, so failed OpenFGA writes stay retryable instead of being skipped as completed duplicates.

Focus:

- Define representative transactional scenarios for the core service write paths.
- Measure contention, connection-pool behavior, and read/write routing through [[projects/rustycog/references/rustycog-db]] and [[entities/db-connection-pool]].
- Confirm that transactions preserve domain invariants under concurrent writes.
- Add remaining rollback and concurrency regression tests for IAMRusty, Telegraph, and additional Manifesto/Hive flows.
- Capture bottlenecks as concrete schema, query, transaction-boundary, or pool-tuning changes.

Done means the team has repeatable evidence for expected transactional load, known failure thresholds, rollback behavior on mid-flow failure, and a short list of changes needed before higher-volume workflows depend on the model.

### IAM external-provider adapter service

The next IAM architecture opportunity is extracting external provider integrations from [[projects/iamrusty/iamrusty|IAMRusty]] into an independent service boundary. IAM should keep the same internal API shape for OAuth/provider linking, but delegate provider-specific calls to adapter services that can be added, deployed, and scaled without changing the IAM service.

Focus:

- Define a stable provider-adapter API for operations such as authorization URL creation, callback token exchange, profile lookup, email lookup, and token refresh.
- Move provider-specific implementations like GitHub and GitLab out of IAMRusty into independently deployable adapter modules or services.
- Keep IAMRusty responsible for identity ownership, user linking, token persistence, and auth decisions; keep provider adapters responsible only for external-provider protocol details.
- Make adding a new external provider a deployment/configuration operation rather than an IAM code change.
- Reuse the existing provider-linking model documented in [[projects/iamrusty/concepts/oauth-provider-linking]] and the extension workflow in [[projects/iamrusty/skills/extending-iamrusty-with-oauth-providers]] as the migration baseline.

Done means IAMRusty can call a stable adapter contract while new providers are introduced by registering a new external adapter implementation, not by editing IAMRusty domain/application logic.

### RustyCog Events outbox pattern

The third focus area is adding an outbox pattern to [[projects/rustycog/references/rustycog-events|RustyCog Events]]. The current event publisher abstraction supports transport selection and SQS fanout, but domain writes still need a durable bridge between database commits and event dispatch.

```mermaid
flowchart TD
    serviceUow["Service Unit Of Work"] --> dbTxn["Db Write Transaction"]
    dbTxn --> domainRows["Domain Rows"]
    dbTxn --> outboxRows["Outbox Rows"]
    outboxWorker["Embedded Outbox Worker"] --> outboxRows
    outboxWorker --> eventPublisher["EventPublisher"]
    eventPublisher --> queue["SQS Or Kafka"]
```

Progress:

- `[[projects/rustycog/references/rustycog-outbox]]` now owns the shared outbox migration, recorder, stored-event representation, and dispatcher loop.
- Manifesto project creation records `ProjectCreated` inside the project-creation transaction, then dispatches it asynchronously after commit.
- Regression tests cover pending outbox persistence, rollback of domain rows plus outbox intent, publish failure retry state, and publish success marking.

Focus:

- Roll the pattern beyond Manifesto `ProjectCreated` once the first slice is stable.
- Define retry, dead-letter, and observability expectations before treating no-op fallback as acceptable behavior.
- Keep service code transport-agnostic, with RustyCog owning the shared outbox mechanics.

Done means a service can commit a domain change and its corresponding event atomically, then dispatch the event asynchronously without losing it when process, network, or queue setup failures happen between commit and publish.

## Roadmap Shape

The workstreams reinforce each other:

- Sentinel Sync tests prove that downstream authorization state can be rebuilt from events.
- Transaction-ready workflows prove that upstream domain writes hold under concurrency and rollback cleanly on mid-flow failure.
- The RustyCog Events outbox pattern connects those two guarantees by making event publication durable.
- IAM external-provider adapters keep identity ownership centralized while letting provider integrations evolve independently.

Together, these features move AIForAll toward a more reliable event-driven platform: services own their data, events carry the integration contract, and authorization state remains synchronized through tested, repeatable infrastructure.

## Related Notes

- [[projects/aiforall/aiforall]]
- [[concepts/event-driven-microservice-platform]]
- [[concepts/integration-testing-with-real-infrastructure]]
- [[projects/sentinel-sync/sentinel-sync]]
- [[projects/iamrusty/iamrusty]]
- [[projects/iamrusty/concepts/oauth-provider-linking]]
- [[projects/iamrusty/skills/extending-iamrusty-with-oauth-providers]]
- [[projects/rustycog/references/rustycog-db]]
- [[projects/rustycog/references/rustycog-events]]
- [[projects/rustycog/references/rustycog-outbox]]
