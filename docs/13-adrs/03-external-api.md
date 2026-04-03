# ADR-03 — External API Protocol

**Status:** Proposed  
**Date:** 2026-04-03  
**Authors:** Intent Rebase Engine Team  
**Phase:** Phase 0–1  

---

## Context

IRE exposes interfaces to:
- **Internal frontend** — Next.js console for operators
- **Runtime platforms** — Temporal, Prefect, custom event loops
- **External systems** — webhooks, CI/CD integrations, audit exporters
- **AI agents** — programmatic intent submission and rebase queries

API design must support:
- Intent CRUD (create, read, update, list, diff)
- Rebase preview and apply operations
- Audit event ingestion and export
- Webhook delivery for event-driven integrations

---

## Decision

**REST API as primary external interface, with Webhooks for event delivery.**

### REST API — Intent and Control Plane

| Endpoint Pattern | Method | Purpose |
|-----------------|--------|---------|
| `/api/v1/intents` | POST | Create new intent |
| `/api/v1/intents/{id}` | GET | Get intent (with version query param) |
| `/api/v1/intents/{id}/versions` | GET | List intent versions |
| `/api/v1/intents/{id}/diff` | POST | Compute semantic diff between versions |
| `/api/v1/intents/{id}/rebase-preview` | POST | Preview rebase plan |
| `/api/v1/intents/{id}/rebase-apply` | POST | Apply rebase directive |
| `/api/v1/artifacts` | GET | List artifacts (filterable by intent, version) |
| `/api/v1/artifacts/{id}` | GET | Get artifact with provenance |
| `/api/v1/audit/events` | GET | Query audit events |
| `/api/v1/health` | GET | Health check |

### Webhooks — Event Delivery

IRE delivers events to external subscribers via webhook:

| Event Type | Trigger |
|-----------|---------|
| `intent.created` | New intent version created |
| `intent.updated` | Intent mutated |
| `rebase.detected` | Semantic diff exceeds threshold |
| `rebase.preview` | Rebase plan generated |
| `rebase.applied` | Rebase directive executed |
| `approval.required` | Rebase requires human approval |
| `artifact.invalidated` | Artifact quarantined |

Webhook payload structure defined in `../04-api/03-webhooks.md`.

### Authentication

- **API keys** for machine-to-machine integrations
- **OAuth 2.0 / OIDC** for user-facing console access
- Tenant isolation enforced via key/scopes prefixing

---

## Consequences

### Positive
- REST is universally understood, easy to debug, good tooling
- Webhooks enable event-driven integrations without polling
- OpenAPI spec can be generated and shared with integrators
- Aligns with existing `04-api/` documentation

### Negative
- REST is synchronous/short-lived; long-running rebase operations need polling or async pattern
- Webhook delivery is at-least-once; clients must handle deduplication

### Neutral
- gRPC considered but deferred (Phase 4 if high-throughput internal calls needed)
- GraphQL considered but deferred (Phase 4 if frontend data-fetching complexity demands it)

---

## Implementation Notes

- Define OpenAPI 3.1 spec in `../04-api/01-rest-api.md`
- Implement webhook delivery with retry (exponential backoff, dead-letter queue)
- All API responses include `X-Request-ID` for tracing
- Tenant ID extracted from auth token, not request path

---

## Related ADRs

- [ADR-01](./01-runtime-adapter.md) — Runtime adapter (internal integration)
- [ADR-04](./04-event-broker.md) — Event streaming (internal)

---

## References

- OpenAPI 3.1: https://spec.openapis.org/oas/v3.1.0
- REST API design: `../04-api/01-rest-api.md`