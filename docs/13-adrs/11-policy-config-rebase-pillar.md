# ADR-11: Policy / Config Rebase Pillar — MVP Design

## Status

Proposed — design only; no implementation commitment

## Context

Agent Safety Rebase has three capability pillars:

1. **Policy / config rebase** (this ADR)
2. Workflow migration / rebase (ADR-09 covers runtime adapter contract)
3. Multi-tenant compliance automation (ADR-08, ADR-10 cover audit and impact)

Currently, the system supports **intent-level rebase** (version diff, preview, apply). Policy and config objects are referenced inside intents but are not first-class rebase targets. A policy change (e.g., updating a rule pack version) should be able to propagate across all intents that reference it, with impact analysis and safety gates.

## Decision

Introduce a **bounded Policy/Config Rebase Pillar MVP** that treats policy and config objects as rebase targets, using the same adapter model and preview flow as intent rebase, but with a distinct input boundary.

### Adapter Model

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
│  PolicyRebaseAdapter                    │
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

The `PolicyRebaseAdapter` is the **only new component** in the MVP. It:
1. Accepts a policy/config change request
2. Identifies all intents that reference the changed policy/config
3. Generates a synthetic `IntentVersionDiff` for each affected intent
4. Delegates to the existing rebase preview/apply pipeline

### Preview Flow

```
POST /policy-snapshots/{snapshot_id}/rebase-preview
  ├── Adapter: resolve snapshot → affected intents
  ├── Adapter: generate synthetic diff per intent
  ├── Existing: compute_rebase_preview_with_graph(intent_id, ...)
  ├── Existing: compensation_action_service.evaluate_policy_gates_for_intent
  └── Returns: PolicyRebasePreviewResponse
               └── contains one ImpactReport per affected intent
```

The preview is **read-only** and **transient**, following the same pattern as `ImpactReport` (ADR-10).

### ImpactReport Output Boundary

The Policy/Config Rebase Pillar **does not introduce a new output format**. It reuses `ImpactReportResponse` (ADR-10) with one addition:

- `policy_snapshot_id` — the source policy snapshot that triggered the rebase
- `affected_intents` — list of intent IDs included in the preview

This keeps the output boundary stable and avoids fragmenting the impact analysis surface.

### API Surface (Proposed, Not Implemented)

- `POST /policy-snapshots/{snapshot_id}/rebase-preview` — preview policy change impact across all referencing intents
- `POST /policy-snapshots/{snapshot_id}/rebase-apply` — apply policy change with the same safety gates as intent rebase
- `GET /policy-snapshots/{snapshot_id}/impact-report` — on-demand read-only ImpactReport for a policy snapshot (reuses ADR-10 pattern)

### Boundaries

- **No new persistence** — reuses `policy_snapshots` table and existing intent repositories
- **No new migration** — adapter operates in-memory; no schema change required for MVP
- **No new executor** — delegates to existing compensation planner + executor
- **No LLM** — policy diff is structural (scope hash comparison), not semantic

### Non-Goals (Explicitly Out of MVP Scope)

- Real-time policy propagation (event-driven auto-rebase)
- Multi-policy atomic rebase (transaction across multiple policy changes)
- Policy/config object versioning lifecycle (separate from intent versioning)
- Environment promotion semantics (dev/staging/prod promotion rules)
- Production-ready claims before intent rebase pillar stabilizes

## Consequences

- **Positive:** Unifies policy and intent rebase under the same safety model; reuses existing compensation, audit, and impact analysis infrastructure; no new DB surface for MVP
- **Negative:** Policy rebase is sequential per-intent (no batch atomicity in MVP); adapter complexity grows with policy object diversity
- **No migration risk:** MVP is adapter + API design only; no schema change

## Related ADRs

- [ADR-07](./07-approval-scope-canonicalization.md): Policy Snapshot canonicalization
- [ADR-09](./09-rebase-apply-rls-transaction-boundary.md): Rebase Apply RLS boundary (reused)
- [ADR-10](./10-impact-report-design.md): ImpactReport output format (reused)

## Implementation Plan (Deferred)

1. **Adapter interface** — define `PolicyRebaseAdapter` trait + in-memory implementation
2. **Handler** — implement `POST /policy-snapshots/{snapshot_id}/rebase-preview`
3. **Route** — wire handler into `build_router`
4. **Tests** — handler-level unit tests + route contract test
5. **OpenAPI** — document proposed endpoints
6. **Docs** — update agent guide with policy rebase flow

No migration, no new repository, no new DB table.
