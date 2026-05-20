---
title: External Provider Sync Jobs
category: concepts
tags: [integrations, sync, organizations, visibility/internal]
sources:
  - Hive/application/src/usecase/sync_job.rs
  - Hive/domain/src/service/sync_service.rs
  - Hive/domain/src/service/external_provider_service.rs
  - Hive/infra/src/external_provider/external_provider_client.rs
  - Hive/config/default.toml
  - hive-events/README.md
  - Hive/openspecs.yaml
summary: Hive links organizations to external providers, validates configurations over HTTP, and starts sync jobs that publish HiveDomainEvent updates.
provenance:
  extracted: 0.77
  inferred: 0.13
  ambiguous: 0.10
created: 2026-04-14T18:56:22.3888182Z
updated: 2026-04-19T11:13:11Z
---

# External Provider Sync Jobs

`[[projects/hive/hive]]` uses external links and sync jobs to pull organization and member state from outside systems. That makes Hive more than an internal org database: it can validate outbound integration configs, create provider links, and kick off long-running sync work that emits domain events for the rest of the platform.

## Key Ideas

- `ExternalProviderServiceImpl` validates that the organization and provider exist, prevents duplicate links, stores provider configuration, and exposes connection-test and link-listing operations.
- `HttpExternalProviderClient` talks to a configured external provider service over HTTP, using endpoints such as `/config/validate`, `/connection/test`, `/organization/info`, and `/members`.
- `SyncServiceImpl::start_sync_job()` refuses to start when sync is disabled or another job is already running for the same external link, so sync job creation is gated by business rules before any outbound work starts.
- Sync jobs can update organization information and fetch external members; the member sync path can generate invitations for discovered users when appropriate.
- `SyncJobUseCaseImpl` publishes `HiveDomainEvent::SyncJobStarted`, and `ExternalLinkUseCaseImpl` publishes `HiveDomainEvent::ExternalLinkCreated`, so outbound integrations are part of Hive's event model as well as its HTTP model.
- Conflict to resolve: `openspecs.yaml` documents a broader external-link and sync REST surface than the live route builder actually exposes, including per-link sync endpoints and more CRUD behavior. ^[ambiguous]

## Open Questions

- Default config points both `iam_service` and `external_provider_service` at `localhost:8080`, which is operationally ambiguous without a stronger environment story. ^[ambiguous]
- The source set shows sync job start and domain execution rules clearly, but not a finished end-to-end operator workflow for monitoring or retrying failed syncs. ^[inferred]

## Sources

- [[projects/hive/hive]] - Service where external links and sync jobs are managed.
- [[projects/hive-events/hive-events]] - Event contract crate carrying sync-related events.
- [[projects/hive/references/hive-runtime-and-configuration]] - Config sections that shape outbound integrations.
- [[projects/hive/references/hive-http-api-and-openapi-drift]] - HTTP and contract mismatch for external-link and sync endpoints.