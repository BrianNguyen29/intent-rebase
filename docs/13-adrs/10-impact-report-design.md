# ADR-10: ImpactReport Design

## Status

Proposed

## Context

Agent Safety Rebase needs a shared primitive that bridges the three capability pillars:
- Policy / config rebase
- Workflow migration / rebase
- Multi-tenant compliance automation

Currently, impact analysis is scattered across:
- `rebase-preview` (diff + risk class)
- `orchestration-dashboard` (side effects + compensation summary)
- `forensic/verify` (feasibility check)

There is no unified, queryable artifact that answers: "If I change this intent/policy/config, what exactly will be affected, and what is the safety status?"

## Decision

Introduce **ImpactReport** as a first-class, versioned domain object.

### Definition

An `ImpactReport` is a read-only, tenant-scoped snapshot produced by the control plane that captures:

1. **Trigger** — what changed (intent version delta, policy snapshot diff, config object diff)
2. **Scope** — which intents, workflows, artifacts, approvals, and side effects are in scope
3. **Invalidation** — which existing artifacts/approvals become invalid
4. **Compensation** — required compensation actions with feasibility and risk class
5. **Safety Gates** — which gates are open, blocked, or need manual review
6. **Provenance** — report ID, generated_at, generated_by, input hashes

### Boundaries

- **Control-plane only** — ImpactReport does not execute changes; it informs decisions
- **Not an LLM output** — computed from structured diff + graph traversal + policy evaluation
- **Versioned** — each report is immutable and linked to the intent versions that produced it

### API Surface (Proposed, Not Implemented)

- `POST /v1/intents/{intent_id}/impact-report` — generate report for current head vs proposed version
- `GET /v1/impact-reports/{report_id}` — retrieve a persisted report
- `GET /v1/intents/{intent_id}/impact-reports` — list reports for an intent

### Relationship to Existing Primitives

| Existing Primitive | Role in ImpactReport |
|--------------------|----------------------|
| Intent diff | Section 1 (Trigger) |
| Dependency graph | Section 2 (Scope) |
| Rebase preview | Sections 3–4 (Invalidation + Compensation) |
| Policy gate | Section 5 (Safety Gates) |
| Audit event | Section 6 (Provenance) |

## Consequences

- **Positive:** Unifies scattered impact analysis into a single queryable artifact; enables UI consolidation; provides audit trail for "why was this change approved"
- **Negative:** Adds a new persisted entity; requires migration and RLS policy; needs careful naming to avoid confusion with `rebase-preview`
- **Migration:** Can initially be an in-memory projection from existing primitives before adding a dedicated table

## Related ADRs

- [ADR-07](./07-approval-scope-canonicalization.md): Approval Scope & Policy Snapshot Canonicalization
- [ADR-09](./09-rebase-apply-rls-transaction-boundary.md): Rebase Apply RLS Transaction Boundary
