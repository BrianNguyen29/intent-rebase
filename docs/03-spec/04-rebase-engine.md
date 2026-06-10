# Rebase Engine Specification

## Purpose

Make structured decisions when an intent changes:
- keep what
- cancel what
- re-request what
- compensate what
- resume from where

## Rebase state machine

```text
DetectedChange
  -> DiffComputed
  -> ImpactComputed
  -> (AutoRepairCandidate | ManualReviewRequired)
  -> RebasePlanIssued
  -> Applied
  -> Verified
  -> Closed
```

## Inputs
- old intent version
- new intent version
- change set
- dependency graph snapshot
- runtime state snapshot
- latest approvals
- policy snapshot
- side effect ledger

## Outputs
- rebase decision
- invalidation set
- review set
- compensation set
- restart boundary
- checkpoint resume pointer
- approval requirements
- operator notices

## Decision classes

### Class A — No-op / Metadata update
Change does not affect execution semantics.

### Class B — Soft review
Does not invalidate immediately, but requires review before the next step.

### Class C — Partial repair
Invalidate locally, keep the rest, rerun from a selected checkpoint.

### Class D — Compensation + repair
Side effects exist that need compensation or mitigation.

### Class E — Hard restart / manual handoff
Not safe enough for auto-repair.

## Rebase algorithm (v1 conceptual)

1. Load intent versions
2. Compute semantic diff
3. Identify impacted clauses
4. Query dependency graph for affected nodes
5. Classify affected nodes by node type and risk policy
6. Detect side-effect class and compensation feasibility
7. Choose candidate checkpoint
8. Re-evaluate approvals and policies
9. Generate rebase plan
10. Optional operator confirmation
11. Apply via runtime adapter
12. Verify resulting execution state

## Checkpoint selection rules
Prefer a checkpoint that is:
- closest
- before the first invalid node
- does not miss mandatory dependencies
- avoids rerunning irreversible side effects unless necessary

## Repair primitives
- drop_task(node)
- rescope_task(node, intent_delta)
- regenerate_artifact(node)
- request_approval(rule)
- insert_validation_step(type)
- insert_compensation(step)
- branch_execution(reason)
- quarantine_output(artifact_id)

## Safety rails
- Do not auto-apply critical changes if the adapter does not support safe pause/resume.
- Do not auto-compensate irreversible side effects.
- Do not reuse stale approvals.
- Do not resume if runtime state and graph state are out of sync.

## Success metrics
- rebase acceptance rate
- percentage work salvaged
- percentage full restarts avoided
- invalidation precision/recall (estimated via review labels)
- incident reduction due to stale intent

## Phase 1 Status

**Phase 1 (Core Control Plane MVP):** ✓ COMPLETE

Implemented components:
- Intent Schema Validation (PR #21)
- Graph HTTP API (PR #22)
- Observability v1 (PR #23)
- Security v1 (PR #24)

### Version Chain Integrity

The rebase engine maintains version chain integrity through `parent_version_id` tracking. Each intent version references its parent, enabling:
- Accurate semantic diff computation between versions
- Reliable rollback to any previous checkpoint
- Complete invalidation set generation when intent changes

For full Phase 1 checklist, see [checklist-phase-1.md](../10-delivery/checklists/checklist-phase-1.md)
