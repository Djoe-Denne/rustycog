---
title: IAMRusty Command Execution
category: references
tags: [reference, commands, reliability, visibility/internal]
sources:
  - IAMRusty/docs/COMMAND_PATTERN.md
  - IAMRusty/docs/COMMAND_RETRY_CONFIGURATION.md
  - IAMRusty/application/src/command/factory.rs
  - IAMRusty/config/test.toml
summary: IAMRusty-specific command-execution notes layered on top of RustyCog's shared registry runtime, especially where retry policy is actively wired into the live service.
provenance:
  extracted: 0.83
  inferred: 0.11
  ambiguous: 0.06
created: 2026-04-14T17:46:37.6929647Z
updated: 2026-04-19T12:08:26.9393504Z
---

# IAMRusty Command Execution

This page assumes the shared command runtime from `[[projects/rustycog/references/rustycog-command]]` and keeps the places where `[[projects/iamrusty/iamrusty]]` configures or uses that runtime differently.

## RustyCog Baseline

- `[[projects/rustycog/references/rustycog-command]]` explains the shared typed registry model, `GenericCommandService`, error mapping, and retry-policy hooks this page builds on.
- `[[concepts/command-registry-and-retry-policies]]` captures the cross-service version of the same runtime pattern.

## Service-Specific Differences

- `CommandRegistryFactory::create_iam_registry` assembles the full auth surface: OAuth login and start-url generation, provider link and relink, provider-token retrieval and revoke, signup, password login, verification, registration completion, username checks, token refresh and JWKS, and password-reset flows.
- Every command is registered with a dedicated handler and an explicit error mapper, which lets HTTP code receive normalized command failures rather than raw lower-layer errors.
- IAMRusty is the clearest example in the vault where the registry is configured directly from `CommandConfig`, so retry behavior can be tuned globally and per command type without changing handler code.
- The docs describe a layered retry hierarchy and environment-specific tuning, and the code path from `config/test.toml` into `CommandRegistryFactory` confirms that the active registry consumes that config directly.
- The current test config sets `max_attempts = 0`, which is stricter than the docs' usual examples of small positive retry counts in tests. ^[ambiguous]
- Some doc examples use human-readable override names such as `login_command`, but the concrete command identifiers in code are names like `password_login`, `oauth_login`, and `generate_relink_provider_start_url`. ^[ambiguous]

## Open Questions

- The docs contain more aspirational retry examples than the active test config uses today, so the practical retry posture varies by environment and may still be evolving. ^[ambiguous]

## Sources

- [[projects/iamrusty/iamrusty]] - Service whose handlers rely on this runtime.
- [[concepts/command-registry-and-retry-policies]] - Distilled concept view of the same mechanism.
- [[projects/iamrusty/references/iamrusty-api-and-auth-flows]] - Handler-level consumers of the command layer.
- [[concepts/structured-service-configuration]] - Config model that supplies registry policy.
- [[projects/rustycog/references/rustycog-command]] - Shared command runtime IAMRusty configures and consumes.
