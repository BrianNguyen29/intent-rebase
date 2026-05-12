# ADR-10: ImpactReport Design

## Status

Accepted — bounded MVP implemented; no persistence, no migration, no production-ready claim

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

Introduce **ImpactReport** as an **on-demand, read-only projection** that aggregates existing primitives at query time.

### Definition

An `ImpactReport` is a transient, tenant-scoped response object produced by the control plane on each request. It captures:

1. **Trigger** — what changed (intent version delta, policy snapshot diff, config object diff)
2. **Scope** — which intents, workflows, artifacts, approvals, and side effects are in scope
3. **Invalidation** — which existing artifacts/approvals become invalid
4. **Compensation** — required compensation actions with feasibility and risk class
5. **Safety Gates** — which gates are open, blocked, or need manual review
6. **Provenance** — report generation timestamp, input version hashes, and generating actor

### Pattern

The MVP follows the same pattern as `OrchestrationDashboardResponse`:
- **Read-only:** the endpoint only queries existing data; it does not create, mutate, or persist a new domain entity.
- **On-demand:** the report is computed from current state every time the endpoint is called.
- **Transient:** there is no `impact_reports` table, no migration, and no RLS policy for a new persisted entity.

### Boundaries

- **Control-plane only** — ImpactReport does not execute changes; it informs decisions
- **Not an LLM output** — computed from structured diff + graph traversal + policy evaluation
- **No persistence for MVP** — the report is a projection, not a stored artifact

### API Surface (Bounded MVP Implemented)

- `GET /intents/{intent_id}/impact-report?tenant_id={tenant_id}&from_version={n}&to_version={m}` — on-demand read-only projection

The endpoint returns a single `ImpactReport` response. No `POST`, `PUT`, `DELETE`, or listing endpoints are in the MVP scope.

### Response Shape (Bounded MVP)

```rust
pub struct ImpactReport {
    pub intent_id: Uuid,
    pub tenant_id: Uuid,
    pub trigger: ImpactTrigger,
    pub scope: ImpactScope,
    pub invalidation: ImpactInvalidation,
    pub compensation: ImpactCompensation,
    pub safety_gates: SafetyGateSummary,
    pub provenance: ImpactProvenance,
    pub unsupported_items: Vec<String>,
}
```

Each subsection is populated by calling existing services:
- `trigger` → intent diff service
- `scope` → graph service
- `invalidation` / `compensation` → rebase preview + compensation planner
- `safety_gates` → policy gate evaluation
- `provenance` → request metadata + input hash

### Non-Goals (Explicitly Out of MVP Scope)

- Persisting ImpactReport to a database table
- Adding a migration, repository, or RLS policy for a new `impact_reports` table
- Caching or memoization of generated reports
- `POST /intents/{intent_id}/impact-report` or any mutation surface
- `GET /impact-reports/{report_id}` retrieval of a stored report
- Event streaming or audit event specifically for report generation (reuses existing audit primitives if needed)

## Consequences

- **Positive:** Unifies scattered impact analysis into a single queryable artifact without adding DB surface; enables UI consolidation; same bounded pattern as orchestration dashboard
- **Negative:** Report is regenerated on every request; no historical lookup without re-execution; caching is deferred to future work
- **No migration risk:** Because the MVP is a pure projection, there is no schema change, no migration, and no RLS policy to validate

## Implementation Plan (Bounded MVP)

1. **Types** — add `ImpactReport` and subsection types to `intent-rebase-types` or `intent-api` types module
2. **Handler** — implement `GET /intents/{intent_id}/impact-report` handler that calls existing services and assembles the projection
3. **Route** — wire the handler into `build_router` in `router.rs`
4. **Tests** — add handler-level unit tests and a route contract test (same pattern as forensic route contract test)
5. **OpenAPI** — document the endpoint and response schema in `docs/04-api/openapi.yaml`
6. **Docs** — update API docs and agent guide to reference the new endpoint

No migration, no new repository, no new DB table.

## Related ADRs

- [ADR-07](./07-approval-scope-canonicalization.md): Approval Scope & Policy Snapshot Canonicalization
- [ADR-09](./09-rebase-apply-rls-transaction-boundary.md): Rebase Apply RLS Transaction Boundary
