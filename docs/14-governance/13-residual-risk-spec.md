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
Acceptance: Accepted by Security Team Lead, review Jan 2026
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

## Related Documents

- [06 — Threat Model v2](./06-threat-model-v2.md)
- [05 — Immutable Retention & Tamper Resistance](./05-immutable-retention-tamper-resistance.md)
- [08 — Tenant Isolation](./08-tenant-isolation.md)