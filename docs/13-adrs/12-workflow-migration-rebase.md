# ADR-12: Workflow Migration / Rebase Pillar — Phase 4 Design

## Status

Proposed — design-only; no implementation, no persistence, no production-ready claim

## Context

Agent Safety Rebase has three capability pillars:

1. Policy / config rebase (ADR-11 covers bounded MVP; full PolicyRebaseAdapter deferred to Phase 4+)
2. **Workflow migration / rebase** (this ADR)
3. Multi-tenant compliance automation (ADR-08, ADR-10 cover audit and impact)

Currently, the system supports **intent-level rebase** (version diff, preview, apply). Workflow-level rebase — where an entire workflow definition changes and all intents referencing it must be evaluated — is not yet addressed. A workflow change (e.g., updating a checkpoint schema, changing a runtime adapter contract) should propagate to all dependent intents with impact analysis and safety gates.

## Decision

**Phase 4 Design (Not Implemented):** Introduce a `WorkflowRebaseAdapter` that bridges workflow definition changes to intent-level rebase operations.

### Design Goals

1. **Workflow-aware impact analysis:** When a workflow definition changes, identify all intents that reference it
2. **Synthetic intent diff generation:** Convert workflow changes into per-intent `IntentVersionDiff` objects
3. **Cross-intent batch preview:** Generate rebase previews for all affected intents in a single operation
4. **Safety gate aggregation:** Combine per-intent safety gates into a workflow-level decision

### API Candidates (Phase 4+ Scope)

**Preview / Apply Endpoints:**
- `POST /workflows/{workflow_id}/rebase-preview` — preview impact across all intents referencing the workflow
- `POST /workflows/{workflow_id}/rebase-apply` — apply workflow change with per-intent safety gates
- `GET /workflows/{workflow_id}/rebase-status` — track batch rebase progress

**Propagation Status Endpoints:**
- `GET /intents/{intent_id}/propagation-status` — bounded stub implemented; full downstream tracking deferred

### WorkflowRebaseAdapter Design

```
┌─────────────────────────────────────────┐
│  Workflow Definition (source of truth)  │
│  - checkpoint schema                    │
│  - runtime adapter contract             │
│  - signal handlers                      │
└─────────────┬───────────────────────────┘
              │
              ▼
┌─────────────────────────────────────────┐
│  WorkflowRebaseAdapter (Phase 4+)       │
│  - converts workflow diff → per-intent  │
│    IntentVersionDiff                    │
│  - identifies affected intents          │
│  - produces synthetic versions          │
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

### Impact on Propagation Status

The `GET /intents/{intent_id}/propagation-status` endpoint (bounded stub delivered) would be extended in Phase 4+ to track:
- Which downstream systems have acknowledged a workflow change
- Per-system last-seen intent version
- Acknowledgment timestamps and failure states

### Boundaries

- **No new persistence for design phase** — reuses existing intent and workflow repositories
- **No new executor** — delegates to existing rebase preview/apply pipeline
- **No LLM** — workflow diff is structural (schema hash comparison), not semantic
- **No mutation in bounded stub** — propagation-status endpoint is read-only

### Non-Goals (Explicitly Out of Scope)

- `WorkflowRebaseAdapter` interface or implementation
- Synthetic `IntentVersionDiff` generation from workflow changes
- Cross-intent batch preview/apply endpoints
- Real-time workflow propagation (event-driven auto-rebase)
- Multi-workflow atomic rebase (transaction across multiple workflow changes)
- Workflow definition versioning lifecycle (separate from intent versioning)
- Environment promotion semantics (dev/staging/prod promotion rules)
- Production-ready claims before intent rebase pillar stabilizes

## Consequences

- **Positive:** Provides a design direction for workflow-level rebase without committing to implementation; reuses existing rebase infrastructure
- **Negative:** Only a design artifact; cross-intent workflow impact remains unimplemented; no workflow-specific diff or synthetic version generation
- **No migration risk:** Design phase is docs-only; no schema change

## Related ADRs

- [ADR-09](./09-rebase-apply-rls-transaction-boundary.md): Rebase Apply RLS boundary (reused)
- [ADR-10](./10-impact-report-design.md): ImpactReport output format (reused)
- [ADR-11](./11-policy-config-rebase-pillar.md): Policy / Config Rebase Pillar — bounded MVP pattern to mirror
- [Propagation Status Implementation Plan](../10-delivery/19-propagation-status-implementation-plan.md): Concrete staged plan for evolving the bounded stub to real downstream tracking

## Implementation Plan

### Phase 4+ (Deferred)

1. **Adapter interface** — define `WorkflowRebaseAdapter` trait + in-memory implementation
2. **Handler** — implement `POST /workflows/{workflow_id}/rebase-preview`
3. **Handler** — implement `POST /workflows/{workflow_id}/rebase-apply`
4. **Route** — wire handlers into `build_router`
5. **Tests** — handler-level unit tests + route contract test
6. **OpenAPI** — document proposed endpoints
7. **Docs** — update agent guide with workflow rebase flow
8. **Propagation status** — extend `GET /intents/{intent_id}/propagation-status` with real downstream tracking

No migration, no new repository, no new DB table for the design phase.

## Acceptance Criteria (Phase 4+ Implementation)

- [ ] `WorkflowRebaseAdapter` trait defined with clear input/output contracts
- [ ] `POST /workflows/{workflow_id}/rebase-preview` returns per-intent preview summaries
- [ ] `POST /workflows/{workflow_id}/rebase-apply` respects per-intent safety gates
- [ ] Route contract tests verify endpoints are wired and reachable
- [ ] OpenAPI spec documents all new endpoints and schemas
- [ ] Docs updated with workflow rebase flow and boundaries
- [ ] Propagation status endpoint returns real downstream tracking data
