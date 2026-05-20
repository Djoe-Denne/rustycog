---
title: Descriptor-Driven Communications
category: concepts
tags: [communication, templates, events, visibility/internal]
sources:
  - Telegraph/domain/src/service/communication_factory.rs
  - Telegraph/infra/src/template/tera_service.rs
  - Telegraph/resources/communication_descriptor/user_signed_up.toml
  - Telegraph/resources/communication_descriptor/password_reset_requested.toml
  - Telegraph/resources/communication_descriptor/user_email_verified.toml
  - Telegraph/resources/templates/user_signed_up_email.txt
  - Telegraph/resources/templates/user_email_verified_notification.txt
  - Telegraph/resources/templates/password_reset_requested_email.html
summary: Telegraph builds emails and notifications from event-specific TOML descriptors and Tera templates, with descriptor authoring rules shaping how new event flows are added.
provenance:
  extracted: 0.79
  inferred: 0.13
  ambiguous: 0.08
created: 2026-04-14T18:18:24.0602572Z
updated: 2026-04-19T11:38:52.5746779Z
---

# Descriptor-Driven Communications

`[[projects/telegraph/telegraph]]` does not hardcode user-facing messages inside event handlers. Instead, `CommunicationFactory` loads a TOML descriptor per event type, extracts variables from the event payload, and asks a Tera-backed template service to render the right email or notification body for that mode.

## Key Ideas

- Each event type is expected to have a descriptor file such as `user_signed_up.toml` or `user_email_verified.toml`, and those descriptors decide whether email, notification, or both are available for the event.
- `CommunicationFactory` uses `EventExtractor` output as template variables, so event fields like `email`, `username`, `verification_token`, and `verification_url` become the data model for rendered communication content.
- Email rendering requires a text template and can optionally include a parallel HTML template, while notification rendering uses text output plus a title resolved from descriptor metadata or template variables.
- `TeraTemplateService` merges environment-derived template variables with event variables before rendering, which is how generated links such as the hosted verification URL appear inside rendered emails. ^[inferred]
- Descriptor metadata can override rendered defaults: for example, descriptor-provided `subject` or `title` values take precedence over the generic fallback names in template rendering.
- Setup currently hardcodes `resources/communication_descriptor` as the descriptor directory instead of loading it from configuration, even though template rendering already uses configurable template paths. Conflict to resolve. ^[ambiguous]

## Authoring Notes

- Descriptor filenames should stay aligned with the event type Telegraph looks up at runtime so the queue config, descriptor file, and tests point at one canonical name.
- Email descriptors should always have a text-template story, with HTML treated as an optional companion rather than the only rendering path.
- Notification descriptors need both body text and a stable title story, either directly in descriptor metadata or via template variables that are guaranteed to exist.
- If a new template variable is required, make sure it is supplied by the event payload extraction path or environment-backed template context before relying on it in content.
- Telegraph already makes template directories configurable, but descriptor directories remain hardcoded, so new descriptor files still need to live in the convention the setup crate expects today.

## Open Questions

- `TemplateConfig` defaults its directory to `templates`, while Telegraph's live TOML files point at `resources/templates`; the service works, but the default and the deployed convention are not the same story. ^[ambiguous]
- The descriptor model only supports email and notification sections today, even though Telegraph's config and surrounding docs hint at broader channel ambitions. ^[ambiguous]

## Sources

- [[projects/telegraph/telegraph]] - Service where descriptor-driven rendering is applied.
- [[projects/telegraph/concepts/multi-channel-delivery-modes]] - Delivery-mode model that sits on top of the descriptor system.
- [[projects/telegraph/references/telegraph-event-processing]] - Where descriptors are loaded inside the event pipeline.
- [[projects/telegraph/references/telegraph-runtime-and-configuration]] - Config and path conventions around templates and descriptors.
