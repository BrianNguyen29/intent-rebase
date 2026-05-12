# ADR-11: Policy / Config Rebase Pillar — MVP Design

## Status

Proposed — bounded MVP implemented (`GET /policy-snapshots/{snapshot_id}/impact-report`); full PolicyRebaseAdapter deferred to Phase 4+

## Context

Agent Safety Rebase has three capability pillars:

1. **Policy / config rebase** (this ADR)
2. Workflow migration / rebase (ADR-09 covers runtime adapter contract)
3. Multi-tenant compliance automation (ADR-08, ADR-10 cover audit and impact)

Currently, the system supports **intent-level rebase** (version diff, preview, apply). Policy and config objects are referenced inside intents but are not first-class rebase targets. A policy change (e.g., updating a rule pack version) should be able to propagate across all intents that reference it, with impact analysis and safety gates.

The original design proposed a full `PolicyRebaseAdapter` that would:
- Convert policy diffs into synthetic `IntentVersionDiff` objects
- Identify all intents referencing a changed policy
- Generate per-intent preview/apply pipelines

**Oracle review:** Full `PolicyRebaseAdapter` is blocked by missing policy diff engine, cross-intent policy lookup, and synthetic diff types. These are substantial components that risk scope creep.

## Decision

**Immediate MVP (Implemented):** Introduce a single read-only endpoint `GET /policy-snapshots/{snapshot_id}/impact-report` that:

1. Looks up the policy snapshot by ID
2. Validates tenant ownership
3. Extracts the associated `intent_id` from the snapshot
4. Delegates to existing `build_impact_report_response` (shared with `GET /intents/{intent_id}/impact-report`)
5. Returns a standard `ImpactReportResponse`

This reuses 100% of existing ImpactReport semantics (ADR-10) without introducing new types, persistence, or mutation.

### Immediate MVP Flow

```
GET /policy-snapshots/{snapshot_id}/impact-report?tenant_id={}&from_version={}&to_version={}
  ├── Fetch policy snapshot by ID
  ├── Validate tenant_id matches snapshot.tenant_id
  ├── Extract snapshot.intent_id
  ├── Delegate to build_impact_report_response(intent_id, tenant_id, from_version, to_version)
  └── Returns: ImpactReportResponse
```

### Full Adapter Model (Deferred to Phase 4+)

```
┌─────────────────────────────────────────┐
│  Policy/Config Object (source of truth) │
│  - rule pack version                    │
│  - scope definition                     │
│  - constraint template                  │
└─────────────┬───────────────────────────┘
              │
              ▼
┌─────────────────────────────────────────┐
│  PolicyRebaseAdapter (Phase 4+)         │
│  - converts policy diff → IntentDiff    │
│  - produces synthetic IntentVersion     │
│  - reuses existing RebasePlan pipeline  │
└─────────────┬───────────────────────────┘
              │
              ▼
┌─────────────────────────────────────────┐
│  Existing Rebase Preview / Apply        │
│  - compute_rebase_preview_with_graph    │
│  - orchestrator.apply_rebase            │
│  - compensation planner + executor      │
└─────────────────────────────────────────┘
```

The full `PolicyRebaseAdapter` remains a future design. It is NOT in the bounded MVP.

### ImpactReport Output Boundary

The immediate MVP **does not introduce a new output format**. It returns a standard `ImpactReportResponse` (ADR-10) with no additional fields.

This keeps the output boundary stable and avoids fragmenting the impact analysis surface.

### API Surface

**Implemented (bounded MVP):**
- `GET /policy-snapshots/{snapshot_id}/impact-report` — on-demand read-only ImpactReport for a policy snapshot's intent (reuses ADR-10 pattern)

**Deferred (Phase 4+):**
- `POST /policy-snapshots/{snapshot_id}/rebase-preview` — preview policy change impact across all referencing intents
- `POST /policy-snapshots/{snapshot_id}/rebase-apply` — apply policy change with the same safety gates as intent rebase

### Boundaries

- **No new persistence** — reuses `policy_snapshots` table and existing intent repositories
- **No new migration** — no schema change required for MVP
- **No new executor** — delegates to existing ImpactReport builder
- **No LLM** — policy diff is structural (scope hash comparison), not semantic
- **No mutation** — endpoint is read-only

### Non-Goals (Explicitly Out of MVP Scope)

- `PolicyRebaseAdapter` interface or implementation
- Synthetic `IntentVersionDiff` generation
- Cross-intent policy lookup (finding all intents that reference a policy)
- Real-time policy propagation (event-driven auto-rebase)
- Multi-policy atomic rebase (transaction across multiple policy changes)
- Policy/config object versioning lifecycle (separate from intent versioning)
- Environment promotion semantics (dev/staging/prod promotion rules)
- Production-ready claims before intent rebase pillar stabilizes

## Consequences

- **Positive:** Unifies policy snapshot queries with impact analysis without adding DB surface; reuses existing ImpactReport infrastructure; no new types or schemas
- **Negative:** Only shows impact for the single intent associated with a snapshot; cross-intent policy impact remains deferred; no policy-specific diff or synthetic version generation
- **No migration risk:** MVP is a pure delegation endpoint; no schema change

## Related ADRs

- [ADR-07](./07-approval-scope-canonicalization.md): Policy Snapshot canonicalization
- [ADR-09](./09-rebase-apply-rls-transaction-boundary.md): Rebase Apply RLS boundary (reused)
- [ADR-10](./10-impact-report-design.md): ImpactReport output format (reused)

## Implementation Plan

### Immediate MVP (Completed)

1. ✅ **Shared helper** — extract `build_impact_report_response` from `query_handlers.rs` for reuse
2. ✅ **Handler** — implement `get_policy_snapshot_impact_report` in `policy_snapshot_handlers.rs`
3. ✅ **Route** — wire handler into `build_router` in `router.rs`
4. ✅ **Tests** — route contract test + OpenAPI drift guard update
5. ✅ **OpenAPI** — document `GET /policy-snapshots/{snapshot_id}/impact-report`
6. ✅ **Docs** — update route-openapi-contract-map.md

No migration, no new repository, no new DB table.

### Deferred (Phase 4+)

1. **Adapter interface** — define `PolicyRebaseAdapter` trait + in-memory implementation
2. **Handler** — implement `POST /policy-snapshots/{snapshot_id}/rebase-preview`
3. **Route** — wire handler into `build_router`
4. **Tests** — handler-level unit tests + route contract test
5. **OpenAPI** — document proposed endpoints
6. **Docs** — update agent guide with policy rebase flow
