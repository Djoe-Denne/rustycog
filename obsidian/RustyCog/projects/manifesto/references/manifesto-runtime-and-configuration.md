---
title: >-
  Manifesto Runtime and Configuration
category: references
tags: [reference, configuration, projects, visibility/internal]
sources:
  - Manifesto/src/main.rs
  - Manifesto/config/default.toml
  - Manifesto/config/development.toml
  - Manifesto/config/test.toml
  - Manifesto/configuration/src/lib.rs
  - Manifesto/setup/src/app.rs
  - Manifesto/setup/src/config.rs
  - Manifesto/application/src/command/factory.rs
  - Manifesto/docs/rustycog-implementation-and-usage-guide.md
summary: >-
  Code-backed view of Manifesto's live config wiring after remediation: verified auth,
  wired logging/retry/business knobs, explicit queue defaults, and component-service runtime settings.
provenance:
  extracted: 0.88
  inferred: 0.08
  ambiguous: 0.04
created: 2026-04-19T11:49:06.1450368Z
updated: 2026-04-19T18:00:00Z
---

# Manifesto Runtime and Configuration

This page narrows `[[projects/rustycog/references/rustycog-config]]` to the parts that are live and service-specific in `[[projects/manifesto/manifesto]]`.

## RustyCog Baseline

- `[[projects/rustycog/references/rustycog-config]]` explains the shared typed-config and env-prefix model.
- `[[concepts/structured-service-configuration]]` covers the general pattern of config files defining runtime policy and the composition root consuming those typed sections.
- `[[references/rustycog-service-construction]]` gives the generic startup sequence that Manifesto specializes.

## Service-Specific Differences

- `ManifestoConfig` composes `server`, `auth`, `logging`, `command`, `queue`, `database`, `scaleway`, and `service` sections under the `MANIFESTO` prefix.
- `src/main.rs` now uses `manifesto_setup::load_config()` and `manifesto_setup::setup_logging()`, so the service follows the same config-backed startup path described in setup.
- `setup/src/app.rs` creates a multi-queue event publisher from `config.queue` unless tests or alternate boot paths inject a publisher explicitly.
- The same setup path also creates `ApparatusEventConsumer`; it runs alongside the HTTP server only when queue config resolves to a real consumer instead of a no-op.
- `auth.jwt.hs256_secret` is used by the current shared HS256-only bearer-token verifier.
- `logging.level` is consumed in live startup.
- `[command.retry]` is threaded into `ManifestoCommandRegistryFactory`.
- `service.component_service.base_url`, `api_key`, and `timeout_seconds` are used by `ComponentServiceClient`.
- `service.business.*` is used for quotas, pagination defaults, validation limits, and member-removal grace periods.

## Checked-In Queue Posture

The checked-in `default`, `development`, and `test` configs all set:

```toml
[queue]
type = "disabled"
```

That is intentional. Local/test boots stay stable unless queue-backed behavior is explicitly enabled for a given environment.

## Notes

- The checked-in TOML files still lean on defaults for some `service.component_service` and `service.business` values, but those defaults are now consumed by runtime rather than ignored.
- Queue-backed publication and consumption are real features of the live runtime, just not enabled by default in local/test configs.
- IAM still contains separate RS256-capable issuance code, but that is not part of the shared service-side verifier contract Manifesto currently relies on.

## Open Questions

- Should future environment examples surface more non-default `service.component_service` and `service.business` values directly in checked-in TOML, or keep the runtime lean on defaults?

## Sources

- [[projects/manifesto/manifesto]] - Service hub and current MVP framing.
- [[concepts/structured-service-configuration]] - Shared typed-config pattern that Manifesto specializes.
- [[references/rustycog-service-construction]] - Generic RustyCog construction flow that Manifesto follows with service-specific additions.
- [[projects/rustycog/references/rustycog-config]] - Crate-level loader and config primitive details.
- [[projects/manifesto/references/manifesto-service]] - Composition-root and route-surface context for the same runtime.
