# Phase 2b Residual Risk & Phase 3 Deferral Register

> **Purpose:** Catalog all explicit Phase 3 deferrals identified during Phase 2b exit review — items that are intentionally out of Phase 2b scope but required for Phase 3 production hardening. This register supports sign-off by making deferred risks explicit, traceable, and owned.
> **Basis:** Phase 2b exit gate review (2026-04-09). Most items below are explicit `[ ] PHASE 3 ITEM` entries in [checklist-phase-2.md](./checklist-phase-2.md); D-09 and D-10 are bounded-scope Phase 2b limitations derived from checked-item notes that still require Phase 3 follow-up.
> **Status:** `P1 APPROVED — Phase 2b conditionally complete with explicit Phase 3 deferrals; external sign-off received (Product Owner ✅, Security ✅, Runtime Integration ✅); Brian Nguyen sole signer (personal project) — 2026-04-28`

---

## Deferral Register

| ID | Deferred Capability | Why Deferred | Owning Proposal | Risk If Delayed | Sign-off Note |
|----|---------------------|--------------|-----------------|-----------------|---------------|
| D-01 | Artifact quarantine: S3 move to quarantine path | Requires artifact-service with S3 integration (not yet delivered) | P2 (Phase 3 Batch 2 — Observability + SRE) | Invalidated artifacts remain in original S3 path; quarantine metadata is recorded but no actual isolation. Artifacts not moved during rebase until Phase 3 artifact-service is wired. | **Acknowledge:** S3 quarantine move is Phase 3. No artifacts are actually moved to quarantine during Phase 2b rebase. Phase 3 artifact-service integration is required before quarantine is enforceable in production. |
| D-02 | Artifact release from quarantine (if rebase resolved) | Requires artifact-service with S3 integration | P2 | No automated release path; artifacts remain in quarantine after rebase resolves until Phase 3. Manual S3 intervention required if release needed before Phase 3. | **Acknowledge:** No automated release mechanism exists. If rebase resolves before Phase 3 artifact-service delivery, artifacts stay in quarantine unless manually released via direct S3 operation. |
| D-03 | Artifact permanent deletion (if rebase requires discard) | Requires artifact-service with S3 integration; requires `security-reviewer` role approval | P2 | Discarded artifacts are not actually deleted from S3; only metadata is updated. Data retained until Phase 3 deletion workflow is implemented. Security/data-retention risk if rebase discard is used before Phase 3. | **Acknowledge:** Permanent artifact deletion is not enforceable in Phase 2b. Requires Phase 3 artifact-service with security-reviewer role. Discarded artifacts persist in S3. |
| D-04 | Replay with new intent version (intent version override) | Requires Phase 3 replay/status infrastructure and event streaming support | P4 (Phase 3 Batch 3b — Forensic Replay Bundle) | Current replay endpoint supports cooperative signal-based replay via checkpoint only. Intent version override is not available. Forensic or correction replay requiring version override is not possible until Phase 3. | **Acknowledge:** Only checkpoint-based cooperative replay is available in Phase 2b. Intent version override replay is Phase 3 P4 scope. |
| D-05 | Full replay compatibility (event streaming, replay status tracking) | Requires Phase 3 replay status / event-service infrastructure | P4 | Replay initiation is delivered but replay status tracking and event streaming are not. Full forensic replay capability is limited until Phase 3 P4 is delivered. | **Acknowledge:** Bounded replay initiation exists; full replay status/event-service is Phase 3 P4. |
| D-06 | Full notification delivery (email, webhook, NATS) | NotifierConsumer records notification intents in-memory only; actual external delivery requires Phase 3 infrastructure | P2 | Notifications are recorded as intents but not delivered externally. Users do not receive email/webhook/NATS notifications until Phase 3. Approval flow still creates records but relies on polling. | **Acknowledge:** In-memory notification recording only; no external delivery in Phase 2b. Phase 3 notification infrastructure required for production notification delivery. |
| D-07 | Event schema versioning (v2 migration path) | v1 → v2 migration deferred to Phase 3 | P2 | All events use v1 schema. If breaking schema changes are needed before Phase 3, v2 migration would need to be expedited. Downstream consumers locked to v1 until Phase 3 migration. | **Acknowledge:** Events are v1 schema only. v2 migration is Phase 3. If schema evolution is needed before Phase 3, coordinate migration explicitly. |
| D-08 | Dead-letter queue (DLQ) for failed event processing | Requires Phase 3 event-service with DLQ infrastructure | P2 | Failed event processing fails open (audit succeeds even if streaming fails). No DLQ means failed events are not retried or held for investigation until Phase 3. Silent failure risk in event streaming path. | **Acknowledge:** DLQ is not implemented in Phase 2b. Failed events fail open — audit is the source of truth; streaming failures are not retried or held. DLQ required before production-grade event streaming. |
| D-09 | Full NATS-based event consumers (startup wiring, DLQ, retry, consumer groups) | Bounded in-memory consumer infrastructure used for testing only; Phase 2b intentionally scoped to abstraction layer | P2 | Only in-memory consumer buffer available for testing. No persistent NATS consumers, no consumer groups, no retry/DLQ. Production event-driven checkpoint creation and notification delivery require Phase 3 consumer infrastructure. | **Acknowledge:** In-memory consumer infrastructure is for testing only. Phase 3 NATS consumer infrastructure required before event-driven checkpoint creation or notification delivery is production-ready. |
| D-10 | Policy snapshot write/revalidation API (S3 upload, write endpoints) | Phase 2b scope limited to read-only GET endpoints; write/revalidation requires Phase 3 workflow | Future (policy snapshot lifecycle — no assigned proposal) | Policy snapshots can be read but not created or revalidated via API in Phase 2b. Snapshot creation relies on event-driven SnapshotCreatorConsumer with bounded scope data. Full scope accuracy requires Phase 3 intent scope access. | **Acknowledge:** Read-only policy snapshot API in Phase 2b. Write/revalidation requires Phase 3. SnapshotCreatorConsumer is event-driven with scope data limitations noted. |
| D-11 | Trigger/orchestration/replay/RLS wrapping (P1-S5/P3-S5) | Trigger handler-level check delivered (P1-S5e); trigger full transaction wrapping with `begin_with_tenant → insert_request_with_tx → cancel_approved_by_intent_with_tx → commit` delivered as P1-S5f bounded slice; `orchestration_runs` RLS table + `create_run_with_tx` + handler RLS path delivered as bounded orchestration slice; `replay_intent` JWT tenant guard delivered; compensation execute single+batch RLS tx delivered as bounded slice with `side_effect_repo()` accessor and `record_result_with_tx + create_with_tx`; artifact graph RLS tx delivered via `ingest_artifact_with_tx` | P1 (Phase 3 production hardening) | Trigger approval, orchestration run creation, artifact graph writes, and compensation execute result/rollback-record writes have bounded RLS-wrapped paths. Replay now has handler-level JWT tenant rejection. Artifact side-effect recording remains out-of-tx/best-effort for non-RLS path; ADR-08 Option A bounded implemented for SQL/RLS ingest path. `SqlxBundleRepository` exists with DB-level RLS; forensic bundle app-level RLS tx bounded delivered for create/list/download handlers; in-memory/non-RLS fallback preserved. | **Acknowledge:** Trigger handler-level check (P1-S5e), trigger full-tx create+cancel (P1-S5f), orchestration_runs RLS slice, replay_intent guard, compensation execute single+batch RLS tx (P1-S5h), and artifact graph RLS tx are BOUNDED VERIFIED LOCALLY. ADR-08 Option A bounded implemented for SQL/RLS ingest path; non-RLS fallback preserved. `SqlxBundleRepository` exists; forensic bundle app-level RLS tx bounded delivered for create/list/download handlers; in-memory/non-RLS fallback preserved; S3 Object Lock/full replay remain open; no production-ready claim. |

---

## Deferred Items by Owning Proposal

| Proposal | IDs | Summary |
|----------|-----|---------|
| P2 — Phase 3 Batch 2 — Observability + SRE | D-01, D-02, D-03, D-06, D-07, D-08, D-09 | Artifact-service S3 integration, full notification delivery, event schema versioning, DLQ, NATS consumer infrastructure |
| P4 — Phase 3 Batch 3b — Forensic Replay Bundle | D-04, D-05 | Intent version override replay, full replay compatibility |
| P1 — Phase 3 Production Hardening | D-11 | Trigger/orchestration/replay bounded RLS slices delivered; compensation execute/artifact/forensic SQL full wrapping deferred or blocked |
| Future (policy snapshot lifecycle) | D-10 | Policy snapshot write/revalidation API |

---

## Status Vocabulary (Normalized)

All deferral items use the following normalized status values:

| Status | Meaning |
|--------|---------|
| **Open** | Work has not started; deferred to future phase |
| **In Progress** | Work actively underway; bounded slice delivered but not complete |
| **Bounded Delivered** | Bounded slice delivered within current phase scope; full scope deferred to future phase |
| **Conditionally Complete** | Phase exit gate passed with explicit deferrals acknowledged and signed off |
| **Closed** | All obligations met; deferral fully addressed |

> **Note:** `Conditionally Complete` is the valid exit state for a phase with an explicit deferral register. It means the phase is functionally complete for its bounded scope, with remaining work catalogued and signed off.

---

## Exit Criteria

A deferral item is considered **Closed** when:

1. **The capability is delivered** — The original deferred capability (D-01 through D-10) is implemented and passes its acceptance criteria.
2. **Risk is mitigated** — The "Risk If Delayed" column is addressed to a level acceptable to the original sign-off reviewers.
3. **Evidence is recorded** — Implementation evidence is captured in the owning phase's checklist (e.g., Phase 3 Batch N checklist).
4. **Sign-off is updated** — Original sign-off reviewers acknowledge the deferral is resolved, or a designated phase owner closes the item with documented rationale.

**Process for closing a deferral:**

1. Owner marks the item as `Closed` in this register with evidence links.
2. Phase exit gate confirmation form is updated to reflect the resolution.
3. If risk changed materially from original sign-off, re-confirmation from original reviewers is required.

---

## Sign-off Readiness

Phase 2b exit gate is now **CLOSED**. Phase 2b is **conditionally complete** with explicit Phase 3 deferrals — consistent with the Phase 2 exit gate definition in checklist-phase-2.md.

**Sign-off status:** Product Owner ✅ APPROVED | Security ✅ APPROVED | Runtime Integration ✅ APPROVED  
*Brian Nguyen (sole signer, personal project) — 2026-04-28 — see [11-phase-2b-sign-off-packet.md](./11-phase-2b-sign-off-packet.md) Section 5*

**Phase 3 entry: AUTHORIZED**

**Reviewers:** The full sign-off is captured in the [Phase 2b External Sign-Off Packet](./11-phase-2b-sign-off-packet.md) — scope reviewed, evidence package, per-role review questions and acceptance prompts, and final decision capture.

---

## Related Docs

- [Phase 2b Checklist — Runtime-Integrated Rebase](./checklists/checklist-phase-2.md) (source of Phase 3 deferral items)
- [Phase 2b Slice A Evidence Verification](./checklists/checklist-phase-2.md#phase-2b-slice-a--evidence-verification--green-2026-04-11)
- [Phase 3 Hardening Plan](./05-phase-3-hardening.md)
- [10 Completion Proposals Tracker](./09-completion-proposals-tracker.md)
- [Current Project Status](./00-current-status.md)
