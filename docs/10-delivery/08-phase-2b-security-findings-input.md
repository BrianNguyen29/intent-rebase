# Phase 2b Findings Input for Phase 3 Security Work

**Status:** Preliminary Batch 0 input  
**Purpose:** Capture the Phase 2b implementation realities that should feed Threat Model v2 and residual-risk review.

This document is an engineering input artifact only.

- It is **not** a Security Team sign-off.
- It is **not** a pen-test substitute.
- It is **not** a final residual-risk register.

---

## Findings

### F-01 — Event transport is still bounded and fail-open

- **Severity:** Medium
- **Observation:** Audit event publishing exists as a best-effort seam, but production broker delivery, retries, consumer groups, and DLQ handling are not yet implemented.
- **Phase 3 implication:** Compensation and forensic workflows must not assume durable end-to-end event transport until the JetStream path is real.
- **Recommended action:** Make production NATS/DLQ/retry work an explicit prerequisite for event-driven production workflows.

### F-02 — Notification behavior is still advisory only

- **Severity:** Medium
- **Observation:** The current notifier records in-memory notification intent only; external delivery is not present.
- **Phase 3 implication:** Approval and compensation operator workflows cannot assume humans are actually notified.
- **Recommended action:** Add delivery semantics, retry, and operator-visible failure handling before relying on notifications operationally.

### F-03 — Artifact quarantine is metadata/status only

- **Severity:** High
- **Observation:** Artifact invalidation and quarantine status exist as bounded metadata/graph state, but real storage movement, release, and deletion are deferred.
- **Phase 3 implication:** Forensic and compensation features must distinguish between logical invalidation and actual artifact custody.
- **Recommended action:** Implement artifact-service or equivalent storage boundary before claiming quarantine integrity.

### F-04 — Replay semantics remain bounded cooperative replay

- **Severity:** Medium
- **Observation:** Replay currently uses bounded checkpoint-based cooperative replay rather than a full runtime-native reset or full compatibility tracking path.
- **Phase 3 implication:** Forensic replay and incident investigation must not assume exact runtime reset semantics yet.
- **Recommended action:** Keep forensic replay isolated and clearly bounded until compatibility guarantees are stronger.

### F-05 — Policy snapshots exist, but snapshot generation is still bounded

- **Severity:** Medium
- **Observation:** Snapshot persistence/read APIs exist and the bounded consumer can create snapshots from event payloads, with default fallbacks when full scope data is missing.
- **Phase 3 implication:** Approval integrity and forensic evidence quality depend on strengthening snapshot generation inputs.
- **Recommended action:** Tighten snapshot creation sources before using snapshots as high-confidence evidence artifacts.

### F-06 — Tenant isolation is documented more strongly than it is enforced

- **Severity:** High
- **Observation:** Tenant isolation strategy is well documented across DB/API/S3/NATS, but verification tests and full enforcement layers are still absent.
- **Phase 3 implication:** Cross-tenant leakage remains a top risk area for compensation, forensic export, and audit access.
- **Recommended action:** Prioritize tenant isolation verification tests and tenant-scoped service enforcement in Phase 3.

### F-07 — Phase 3 service boundaries are still emerging

- **Severity:** Medium
- **Observation:** `compensation-service` and `forensic-service` now exist as scaffolds, while `artifact-service` and `tenant-service` remain implied rather than implemented.
- **Phase 3 implication:** Threat modeling and risk review should treat service boundaries as moving targets during early Phase 3 implementation.
- **Recommended action:** Revisit trust boundaries after Batch 1 crate/service responsibilities are concrete.

---

## Proposed Threat Model v2 Inputs

These findings should feed the next update of `docs/14-governance/06-threat-model-v2.md` in these areas:

- event injection, message loss, and consumer lag,
- notification failure and operator-awareness gaps,
- artifact custody and quarantine integrity,
- replay trust boundaries,
- snapshot evidence integrity,
- multi-tenant isolation failures,
- emerging inter-service trust boundaries.

---

## Proposed Residual Risk Candidates

These are candidates for later formalization in `docs/14-governance/13-residual-risk-spec.md`:

- delayed detection of event delivery failures,
- incomplete artifact custody guarantees,
- bounded replay mismatch vs forensic expectations,
- cross-tenant data exposure through incomplete enforcement,
- snapshot incompleteness under degraded event payloads.
