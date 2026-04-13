# 13 — Residual Risk Specification

**Status:** Proposed  
**Phase:** Phase 3  
**Owner:** Security Team

---

## Mục đích

Define the process for identifying, documenting, and accepting residual risks — risks that remain after all mitigations are applied. Ensures:
- **Transparency** — known risks are not hidden
- **Accountability** — risk acceptance requires explicit approval
- **Auditability** — residual risks are tracked and reviewed

---

## Risk Acceptance Process

```
1. Identify risk (from threat model, pen test, or incident)
2. Assess risk (likelihood × impact)
3. Define mitigations
4. Implement mitigations
5. Calculate residual risk
6. If residual risk > acceptable threshold:
   - Either implement additional mitigations, or
   - Accept risk (with explicit approval)
7. Document accepted risk
8. Review periodically
```

---

## Risk Assessment Framework

### Risk Scores

| Score | Level | Description |
|-------|-------|-------------|
| 1-3 | Low | Minimal impact; acceptable |
| 4-6 | Medium | Moderate impact; mitigations required |
| 7-9 | High | Significant impact; senior approval needed |
| 10 | Critical | Severe impact; must mitigate |

### Risk Matrix

| | Low Likelihood | Medium Likelihood | High Likelihood |
|---|----------------|-------------------|-----------------|
| **High Impact** | Medium | High | Critical |
| **Medium Impact** | Low | Medium | High |
| **Low Impact** | Low | Low | Medium |

---

## Residual Risk Register

### Format

```json
{
  "risk_id": "RR-001",
  "title": "Audit chain gap detection latency",
  "description": "Hash chain verification runs daily, not real-time; tampering in gap period may not be detected until next run",
  "threat_category": "Tamper detection",
  "likelihood": "Low",
  "impact": "High",
  "risk_score": 6,
  "mitigations": [
    {
      "control": "Daily automated verification",
      "implemented": true,
      "effectiveness": "Medium"
    },
    {
      "control": "Real-time anomaly detection on audit events",
      "implemented": false,
      "planned": "Phase 4"
    }
  ],
  "residual_likelihood": "Low",
  "residual_impact": "Medium",
  "residual_score": 4,
  "acceptance": {
    "accepted": true,
    "accepted_by": "CISO",
    "acceptance_date": "2025-04-03",
    "review_date": "2026-04-03",
    "rationale": "Daily verification sufficient for current threat model; real-time detection adds significant complexity"
  }
}
```

---

## Risk Categories for IRE

| Category | Examples |
|----------|----------|
| **Data Integrity** | Hash chain gaps, snapshot tampering, graph corruption |
| **Confidentiality** | Cross-tenant data leakage, PII exposure |
| **Availability** | Service disruption, freeze duration impact |
| **Compliance** | Unmet retention requirements, audit gaps |
| **Operational** | Human error in approval, misconfigured rules |
| **Third-Party** | Runtime adapter compromise, cloud provider issues |

---

## Acceptance Criteria

### Low Risk (Score 1-3)

- Auto-accepted by system
- Documented in risk register
- Review annually

### Medium Risk (Score 4-6)

- Team lead approval required
- Documented with mitigation plan
- Review quarterly

### High Risk (Score 7-9)

- CISO or delegate approval required
- Documented with remediation timeline
- Review monthly

### Critical Risk (Score 10)

- Must be mitigated before production
- No acceptance allowed in production

---

## Review Cadence

| Risk Level | Review Frequency |
|------------|-------------------|
| Low | Annual |
| Medium | Quarterly |
| High | Monthly |
| Critical | Weekly |

---

## Risk Register Example Entries

### RR-01: Hash Chain Verification Latency

```
Risk: Hash chain verification runs daily; tampering may go undetected for up to 24 hours
Likelihood: Low
Impact: High
Score: 6 (Medium-High)

Mitigations:
- Daily automated verification job (implemented)
- Anomaly detection on audit event patterns (planned Phase 4)

Residual Score: 4 (Medium)
Acceptance: Accepted by CISO, review April 2026
```

### RR-02: Service Account Key Rotation Gaps

```
Risk: Service account credentials may be rotated infrequently, increasing compromise window
Likelihood: Medium
Impact: High
Score: 8 (High)

Mitigations:
- 90-day mandatory rotation policy (implemented)
- Automated rotation via secret manager (planned Phase 3)

Residual Score: 5 (Medium)
Acceptance: Accepted by Security Team Lead (CISO delegate), review Jan 2026
Review: Monthly
Rationale: High residual score warrants close monitoring; 90-day rotation reduces
           compromise window; automated rotation in Phase 3 further reduces risk
```

### RR-03: Multi-Tenant Resource Quota Circumvention

```
Risk: Resource quota can be temporarily exceeded via burst operations
Likelihood: Low
Impact: Medium
Score: 3 (Low)

Mitigations:
- Per-tenant rate limiting at API gateway (implemented)
- Quota soft-limits with alerting (implemented)

Residual Score: 2 (Low)
Acceptance: Auto-accepted, documented
```

---

### RR-04: Event Delivery Failure Detection Latency

```
Risk: Audit event publishing is best-effort; production broker delivery, retries,
consumer groups, and DLQ handling are not yet implemented. Compensation and forensic
workflows must not assume durable end-to-end event transport until the JetStream path
is real.
Source: F-01 (Phase 2b Findings)
Threat Category: Data Integrity / Availability
Likelihood: Medium
Impact: High
Risk Score: 8 (High)

Mitigations:
- Daily automated hash-chain verification job (implemented)
- Real-time anomaly detection on audit event patterns (planned Phase 4)

Residual Score: 5 (Medium)
Acceptance: Pending — gated on JetStream/DLQ production delivery
Review: Monthly
Rationale: Until JetStream/DLQ delivery is production-ready, event loss risk
           requires monthly reassessment; daily hash-chain verification provides
           secondary detection but does not substitute for durable transport
```

---

### RR-05: Operator Notification Not Actually Delivered

```
Risk: The notifier records in-memory notification intent only; external delivery,
retry, and operator-visible failure handling are not present. Approval and
compensation operator workflows cannot assume humans are actually notified.
Source: F-02 (Phase 2b Findings)
Threat Category: Operational
Likelihood: Medium
Impact: High
Risk Score: 8 (High)

Mitigations:
- In-memory notification intent recording (implemented)
- Operator-visible failure handling at the notifier layer (planned Phase 3)

Residual Score: 5 (Medium)
Acceptance: Pending — gated on notification delivery semantics implementation
Review: Monthly
Rationale: Until external delivery semantics are implemented, approval/compensation
           workflows cannot rely on human notification; monthly review ensures
           progress tracking toward Phase 3 delivery implementation
```

---

### RR-06: Artifact Custody Not Actual (Metadata Only)

```
Risk: Artifact invalidation and quarantine exist as bounded metadata/graph state, but
real storage movement, release, and deletion are deferred. Forensic and compensation
features must distinguish between logical invalidation and actual artifact custody.
Source: F-03 (Phase 2b Findings)
Threat Category: Data Integrity / Operational
Likelihood: Medium
Impact: High
Risk Score: 8 (High)

Mitigations:
- Logical quarantine status tracking in graph state (implemented)
- Artifact-service or equivalent storage boundary (planned Phase 3/4)

Residual Score: 6 (Medium) — logical quarantine separates intent from artifact
Acceptance: Accepted by Security Team Lead, review Monthly
Review: Monthly
Rationale: Logical quarantine sufficient for graph-level integrity; storage custody
           deferred pending artifact-service implementation. Monthly review tracks
           Phase 3/4 progress toward actual custody implementation
```

---

### RR-07: Forensic Replay Bounded to Cooperative Checkpoint Replay

```
Risk: Replay uses bounded checkpoint-based cooperative replay rather than full
runtime-native reset or full compatibility tracking path. Forensic replay and
incident investigation must not assume exact runtime reset semantics yet.
Source: F-04 (Phase 2b Findings)
Threat Category: Data Integrity / Operational
Likelihood: Medium
Impact: Medium
Risk Score: 6 (Medium)

Mitigations:
- Forensic replay isolated and clearly bounded (implemented)
- Full compatibility tracking path (planned Phase 4)

Residual Score: 4 (Medium)
Acceptance: Accepted by Security Team Lead, review Quarterly
Review: Quarterly
Rationale: Bounded cooperative replay sufficient for Phase 3 forensic use cases;
           exact runtime reset deferred to Phase 4
```

---

### RR-08: Snapshot Evidence Integrity Under Degraded Event Payloads

```
Risk: Snapshot persistence/read APIs exist, and the bounded consumer creates snapshots
from event payloads with default fallbacks when full scope data is missing. Approval
integrity and forensic evidence quality depend on strengthening snapshot generation
inputs.
Source: F-05 (Phase 2b Findings)
Threat Category: Data Integrity / Compliance
Likelihood: Low
Impact: High
Risk Score: 6 (Medium)

Mitigations:
- Snapshot persistence/read APIs (implemented)
- Bounded consumer snapshot creation with default fallbacks (implemented)
- Snapshot creation sources to be tightened (planned Phase 3/4)

Residual Score: 4 (Medium) — fallback snapshots may reduce evidence confidence
Acceptance: Accepted by Security Team Lead, review Quarterly
Review: Quarterly
Rationale: Fallback snapshots acceptable for Phase 3; source tightening before
           high-confidence evidence artifact use
```

---

### RR-09: Cross-Tenant Data Exposure Through Incomplete Enforcement

```
Risk: Tenant isolation strategy is documented across DB/API/S3/NATS, but verification
tests and full enforcement layers are still absent. Cross-tenant leakage remains a top
risk area for compensation, forensic export, and audit access.
Source: F-06 (Phase 2b Findings)
Threat Category: Confidentiality
Likelihood: High
Impact: High
Risk Score: 9 (High)

Mitigations:
- Tenant isolation strategy documented across all surfaces (implemented)
- Per-tenant rate limiting at API gateway (implemented)
- Quota soft-limits with alerting (implemented)
- Tenant isolation verification tests and tenant-scoped enforcement (planned Phase 3)

Residual Score: 6 (Medium) — enforcement gaps closed via Phase 3 verification
Acceptance: Accepted by CISO, review Oct 2026
Review: Monthly (policy); CISO-granted exception to quarterly review acceptable
        pending Phase 3 Batch 3a completion — reassess if Batch 3a slips
Rationale: Verification tests and enforcement layers in Phase 3 Batch 3a;
           cross-tenant leakage is highest-priority mitigation area.
           Exception granted: Phase 3 Batch 3a delivery is actively in progress
           and expected to close enforcement gaps before Oct 2026 review date.
           If Batch 3a slips, review cadence reverts to monthly.
```

---

### RR-10: Moving Trust Boundaries During Phase 3

```
Risk: compensation-service and forensic-service exist as scaffolds, while
artifact-service and tenant-service remain implied rather than implemented.
Threat modeling and risk review should treat service boundaries as moving targets
during early Phase 3 implementation.
Source: F-07 (Phase 2b Findings)
Threat Category: Third-Party / Operational
Likelihood: Medium
Impact: Medium
Risk Score: 6 (Medium)

Mitigations:
- Service boundaries treated as provisional during Phase 3 Batch 1 (implemented)
- Trust boundary reassessment after Batch 1 crate/service responsibilities concrete
  (planned Phase 3 Batch 2)

Residual Score: 4 (Medium)
Acceptance: Auto-accepted, documented
Review: Quarterly
Rationale: Moving boundaries are an expected Phase 3 early-state reality;
           reassessment cadence established
```

---

## Related Documents

- [06 — Threat Model v2](./06-threat-model-v2.md)
- [05 — Immutable Retention & Tamper Resistance](./05-immutable-retention-tamper-resistance.md)
- [08 — Tenant Isolation](./08-tenant-isolation.md)
- [08 — Phase 2b Security Findings Input](../10-delivery/08-phase-2b-security-findings-input.md)