# Phase 3 Dependency Audit

> **Note:** The `07-` prefix in this filename is a legacy sequence number. It overlaps with `07-staffing-and-roles.md` in the same directory; the numbering is non-semantic and does not indicate ordering or dependency.

**Status:** Prepared during Batch 0  
**Purpose:** Record the cross-service assumptions that Phase 3 implementation depends on, so Batch 1+ work does not silently rely on Phase 2 bounded placeholders.

---

## Summary

This audit separates assumptions into three buckets:

- **Verified groundwork exists** — a bounded seam or domain concept already exists in Phase 2.
- **Provisional assumption** — docs or ADRs imply a direction, but the production implementation is not yet present.
- **Not started** — Phase 3 depends on a capability that does not yet exist in code.

Batch 0 outcome:

- scaffold crates now exist for compensation and forensic work,
- the most important Phase 3 dependencies are documented,
- later slices should treat all provisional and not-started items as explicit implementation targets, not hidden assumptions.

---

## 1. Eventing and Consumer Infrastructure

| Dependency | Current State | Status | Notes |
|---|---|---|---|
| Event publisher abstraction | `intent-rebase-types` provides bounded event publisher seam | Verified groundwork | Phase 2 bounded event publishing exists; Phase 3 still needs production broker delivery |
| Event subject naming | `audit.events.v1.{tenant_id}.>` direction documented in ADR | Verified groundwork | Subject versioning direction exists; registry/schema governance does not |
| JetStream/TLS/auth | Required by ADR for production NATS cluster | Not started | Phase 2 only has bounded in-memory consumers |
| DLQ stream | ADR requires DLQ for failed delivery | Not started | Explicit Phase 3 infra work |
| Consumer groups | ADR expects horizontal scaling with consumer groups | Not started | Needed before production compensation/forensic consumers |
| Retry policy for failed consumers | Mentioned in checklist/ADR | Not started | Must be defined for compensation and bundle generation |

Relevant references:
- `docs/13-adrs/04-event-broker.md`
- `docs/10-delivery/checklists/checklist-phase-2.md`

---

## 2. Data Model and Persistence Dependencies

| Dependency | Current State | Status | Notes |
|---|---|---|---|
| Policy snapshot model | Exists with bounded persistence/read APIs | Verified groundwork | Useful input to later forensic bundle generation |
| Approval request lifecycle | Exists with bounded status transitions | Verified groundwork | Later compensation/forensic slices can reference these records |
| Artifact invalidation metadata | Exists as bounded graph/status slice | Verified groundwork | Real artifact quarantine/release/delete still missing |
| Side effect ledger schema + repository groundwork | `010_create_side_effects_ledger.sql` and `compensation-service` repo groundwork now exist | Verified groundwork | Persist-and-query groundwork delivered; capture-on-write/API/planner remain open |
| Side effect rollback schema | Checklist references `009_side_effect_rollbacks.sql` | Not started | Required for Batch 1 |
| Optimization indexes | Future optimization migration number TBD | Not started | Required for Batch 4 |

---

## 3. Storage and Bundle Dependencies

| Dependency | Current State | Status | Notes |
|---|---|---|---|
| Tenant-scoped artifact paths | Documented in tenant isolation and artifact notes | Provisional assumption | Path strategy exists in docs, but real artifact-service is missing |
| Quarantine S3 move/release/delete | Explicitly deferred from Phase 2 | Not started | Phase 3 depends on an actual artifact-service/storage implementation |
| Forensic bundle S3 path | Documented as `forensic-bundles/{tenant}/{bundle_id}/` | Provisional assumption | Structure exists in docs, not yet in code |
| Retention/lifecycle policy | Documented for forensic bundles | Provisional assumption | Infra policy not yet configured |
| Bundle download/export surface | Documented in forensic spec | Not started | API and presigned delivery not implemented |

Relevant references:
- `docs/14-governance/10-forensic-bundle.md`
- `docs/14-governance/08-tenant-isolation.md`

---

## 4. Security, SRE, and Governance Dependencies

| Dependency | Current State | Status | Notes |
|---|---|---|---|
| Threat Model v2 | Proposed governance doc exists | Verified groundwork | Needs Phase 2b findings integrated and later review/sign-off |
| Residual risk register process | Proposed governance doc exists | Verified groundwork | Needs concrete entries once implementation advances |
| Phase 3 SLO targets | Baseline SLO doc exists from earlier phases | Provisional assumption | Needs explicit Phase 3 provisional targets and later SRE confirmation |
| Runbooks for compensation/forensic failures | Existing runbook doc has high-level placeholders | Provisional assumption | Needs operational expansion in Batch 2 |
| Tenant isolation verification | Good doc/spec exists | Provisional assumption | Real tests and enforcement still missing |
| Forensic access control roles | Documented in forensic spec | Provisional assumption | RBAC/service enforcement not yet implemented |

---

## 5. Missing Service Boundaries

Phase 3 docs imply several service boundaries that do not yet exist as production code:

- `compensation-service` — now scaffolded, logic still not started
- `forensic-service` — now scaffolded, logic still not started
- `artifact-service` — not present; still implied by quarantine/release/delete work
- `tenant-service` — not present; quota and onboarding/offboarding remain conceptual
- `rule-pack-service` tenant isolation seam — not present as dedicated service boundary

These are not blockers for Batch 0, but they must be treated as explicit implementation scope during Batch 1–4 planning.

---

## Recommended Next Slice Mapping

### Batch 1 should address first

1. Side effect ledger schema and repository
2. Side effect capture on artifact-producing or externally mutating paths
3. Compensation planner/executor skeleton over persisted side effects

### Batch 2 should address first

1. Real consumer topology and retry/DLQ policy
2. Tracing/metrics for compensation and bundle generation
3. Expanded runbook detail

### Batch 3 should address first

1. Forensic bundle persistence/generation
2. Tenant isolation verification tests
3. Quota enforcement and export access control

---

## Batch 0 Audit Completion Condition

This audit is complete for Batch 0 when it is used as the source-of-truth input for later implementation planning.

It does **not** mean the audited dependencies are satisfied.
