# ADR-15 — NATS Per-Tenant JetStream Stream Migration Strategy

## Status

**Proposed — Design only** — Stream migration strategy documented. Implementation of per-tenant streams is blocked on external gates (A-03, A-05). No code changes.

## Context

The current NATS JetStream configuration uses a single shared `audit_events` stream with subject filter `audit.events.v1.>`. All tenants publish audit events to subjects under this stream. As the system scales, this creates several concerns:

1. **No tenant-level retention control** — All tenants share the same retention policy and storage limits.
2. **No tenant-level isolation** — A misconfigured publisher for one tenant could theoretically publish to another tenant's subject space (subjects are convention-based, not ACL-enforced in the current bounded slice).
3. **Operational visibility** — It is difficult to measure per-tenant storage, message rates, or back-pressure when all events flow through a single stream.
4. **Compliance boundaries** — Some compliance regimes may require physical or logical separation of audit logs per tenant.

### Design Question

How should we migrate from a single shared stream to per-tenant streams without data loss, downtime, or duplicate storage?

## Decision

**A staged migration is proposed, with Stage 1 (readiness assessment) complete and Stages 2–4 blocked on external gates.**

### Migration Stages

| Stage | Description | Status | Blocker |
|-------|-------------|--------|---------|
| **Stage 1** | Readiness assessment: tenant UUID inventory, stream naming convention, rollback plan | ✅ Complete | None |
| **Stage 2** | Additive stream creation: create per-tenant streams alongside shared stream; dual-write with env gate | 🟡 Proposed | A-03 (SRE sign-off), A-05 (staging env) |
| **Stage 3** | Cutover: switch reads to per-tenant streams; shared stream becomes read-only | 🔴 Blocked | A-03, A-05, A-04 (security review) |
| **Stage 4** | Decommission shared stream after retention period expires | 🔴 Blocked | A-03, A-05, A-04 |

### Stream Naming Convention

```
Stream name:  audit_events_<tenant_uuid_short>
Subjects:     audit.events.v1.<tenant_uuid>.>
Retention:    limits (configurable per tenant)
Max bytes:    configurable per tenant (default 1GB)
```

Where `<tenant_uuid_short>` is the first 8 characters of the tenant UUID for readability.

### Duplicate Storage Guardrail

**Critical constraint:** Per-tenant streams MUST NOT overlap in subject space with the shared stream, or NATS JetStream will store duplicate copies of the same message.

- Shared stream subjects: `audit.events.v1.>` (current)
- Per-tenant stream subjects: `audit.events.v1.<tenant_uuid>.>` (proposed)

This is technically non-overlapping at the subject level because `audit.events.v1.>` matches `audit.events.v1.<tenant_uuid>.>`. Therefore, during the dual-write period (Stage 2), messages published to `audit.events.v1.<tenant_uuid>.some_event` would be stored in BOTH the shared stream and the per-tenant stream.

**Mitigation options evaluated:**

1. **Subject prefix migration** — Change per-tenant subjects to `audit.events.v2.<tenant_uuid>.>` so they do not match the shared stream's `audit.events.v1.>`. This requires consumer updates and is a breaking change.
2. **Publisher-side env gate** — During Stage 2, publishers write to either the shared subject OR the per-tenant subject, controlled by an environment variable. No duplicate storage, but consumers must read from both during transition.
3. **Consumer mirroring** — Keep a single shared stream but use JetStream mirrors or sources to replicate into per-tenant streams. This is a NATS 2.10+ feature and may not be available in all deployments.

**Recommended option for Stage 2:** Option 2 (publisher-side env gate with subject prefix v2).

- Publishers check `NATS_PER_TENANT_STREAMS=true`.
- If enabled, publish to `audit.events.v2.<tenant_uuid>.<event_type>`.
- If disabled, publish to `audit.events.v1.<tenant_uuid>.<event_type>` (current behavior).
- Consumers read from both `v1` and `v2` subjects during transition.
- After cutover, `v1` subjects are deprecated.

### Rollback Plan

1. If per-tenant streams cause issues, set `NATS_PER_TENANT_STREAMS=false` to revert to shared-stream publishing.
2. Per-tenant streams remain but receive no new messages; they can be deleted after confirmation.
3. The shared stream has continuous retention and never lost messages during the dual-write period.

## Consequences

### Positive
- Per-tenant retention, quotas, and operational visibility
- Tenant-level message isolation
- Compliance-friendly separation of audit logs

### Negative
- More streams to monitor and manage
- Subject prefix migration (`v1` → `v2`) is a breaking change for external consumers
- Increased NATS server memory usage (one stream metadata overhead per tenant)

### Neutral
- Stage 1 readiness assessment is complete (`23-project-assessment-and-execution-tracker.md` I4)
- Stages 2–4 require SRE coordination and cannot proceed locally

## Implementation Notes

**Not implemented (design only):**
- No per-tenant stream code has been added
- No subject prefix migration (`v1` → `v2`) has been performed
- No environment gate exists
- The shared `audit_events` stream remains the sole stream

**Blocked on:**
- A-03: Named external SRE reviewer; signed Section H of external review packet
- A-05: Production NATS topology deployed and validated
- A-04: External security review of per-tenant subject isolation and ACL design

## Evidence

- Current stream initialization: `crates/intent-api/src/nats_jetstream/stream.rs`
- Readiness assessment: `docs/10-delivery/23-project-assessment-and-execution-tracker.md` §I4

## Related ADRs

- ADR-04 (Event Broker): NATS JetStream as primary event broker
- ADR-12 (Workflow Migration): Phase 4 NATS topology requirements

## Review History

| Date | Reviewer | Notes |
|------|----------|-------|
| 2026-06-10 | (fixer) | ADR created; per-tenant stream migration strategy documented; duplicate storage guardrail defined; implementation explicitly deferred to external gates |

---

**Next Step**: External SRE review of stream naming convention, subject prefix migration plan, and NATS server capacity model. Implementation begins only after A-03 and A-05 are unblocked.
