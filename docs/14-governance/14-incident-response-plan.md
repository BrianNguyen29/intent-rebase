# 14 — Incident Response Plan

**Status:** Proposed  
**Phase:** Phase 2+  
**Owner:** Security Team

---

## Mục đích

Defines the end-to-end incident response process for Intent Rebase Engine — covering detection, triage, containment, eradication, recovery, and post-incident review. This plan operates in conjunction with [11-incident-freeze.md](./11-incident-freeze.md) (data freeze procedures) and [10-forensic-bundle.md](./10-forensic-bundle.md) (evidence preservation).

**Scope:** Security incidents affecting confidentiality, integrity, or availability of the Intent Rebase Engine. Not in scope: availability incidents handled purely via SRE runbooks (see [05-runbooks.md](../09-operations/05-runbooks.md)).

---

## Incident Severity Levels

| Severity | Description | Response Time | Examples |
|----------|-------------|--------------|----------|
| **SEV1 — Critical** | Active breach; data exfiltration or active attacker | Immediate | Confirmed data exfil, credential compromise, ransomware |
| **SEV2 — High** | Suspected breach; significant risk of data exposure | < 1 hour | Cross-tenant access detected, audit tampering suspected |
| **SEV3 — Medium** | Security anomaly; investigation required | < 4 hours | Anomalous API patterns, suspicious intent creation |
| **SEV4 — Low** | Minor anomaly; low risk | < 24 hours | Single failed authentication burst, minor policy deviation |

---

## Incident Response Phases

### Phase 1 — Detection

**Who:** Security monitoring, SRE on-call, or any team member

**Actions:**
1. Alert received or anomaly identified
2. Initial assessment: confirm whether event is security-related
3. Assign preliminary severity (SEV1–SEV4)
4. Open incident ticket with severity tag

**Outputs:**
- Incident ticket (e.g., JIRA/Slack incident channel)
- Preliminary severity assignment
- Incident response team notified

**Tools:**
- Alerting rules: `infrastructure/local/prometheus/rules.yml`
- Audit events: `GET /api/v1/audit/events` with tenant_id filter
- SLO dashboard: Grafana intent-rebase-slo

---

### Phase 2 — Triage

**Who:** Security lead + incident commander

**Actions:**
1. Assess scope: what data/systems are affected
2. Determine if tenant(s) are impacted
3. Identify attack surface(s) involved
4. Invoke [data freeze](./11-incident-freeze.md) if evidence preservation is needed
5. Notify affected tenant(s) if data exposure confirmed (GDPR Article 33: within 72 hours)
6. Assign roles:
   - **Incident Commander** — leads response
   - **Technical Lead** — investigation
   - **Communications Lead** — stakeholder updates
   - **Legal/Privacy** — regulatory notification

**Evidence preservation:**
1. Invoke data freeze (`POST /api/v1/incident/freeze`)
2. Capture forensic bundle (`POST /api/v1/forensic/bundle`) — see [10-forensic-bundle.md](./10-forensic-bundle.md)
3. Export audit events for affected tenant/time range
4. Preserve all relevant logs (no rotation/deletion)

**Outputs:**
- Incident commander assigned
- Data freeze invoked (if applicable)
- Forensic bundle initiated
- Stakeholders notified

---

### Phase 3 — Containment

**Who:** Technical lead + Engineering

**Actions:**
1. Isolate affected systems/tenants if necessary
2. Revoke compromised credentials — API key rotation, JWT invalidation
3. Block exploited attack surface temporarily (e.g., disable endpoint)
4. Enable enhanced monitoring for ongoing attack patterns
5. Document all containment actions in audit trail

**Credential Compromise Response:**
1. Rotate all credentials for affected service accounts
2. Invalidate all active sessions for affected tenant(s)
3. Enable MFA reset for affected users
4. Update [authz matrix](./07-authz-matrix.md) if permissions changes needed

**Outputs:**
- Attack surface temporarily hardened
- Compromised credentials rotated
- Enhanced monitoring active
- All actions logged in audit trail

---

### Phase 4 — Eradication

**Who:** Technical lead + Platform Team

**Actions:**
1. Identify root cause of the incident
2. Remove attacker access/artefacts
3. Patch/fix exploited vulnerability
4. Clear any injected/malicious data
5. Verify tenant isolation is restored (cross-tenant incidents)
6. Update [threat model](./06-threat-model-v2.md) with new attack vectors
7. Update [residual risk register](./13-residual-risk-spec.md) if new risks identified

**Outputs:**
- Root cause documented
- Vulnerability remediated
- Threat model updated
- New residual risk entries (if applicable)

---

### Phase 5 — Recovery

**Who:** SRE lead + Engineering

**Actions:**
1. Restore normal operations (gradual, not all at once)
2. Monitor for re-emergence of attack patterns
3. Verify data integrity (hash chain verification)
4. Release data freeze (`DELETE /api/v1/incident/freeze/{freeze_id}`)
5. Restore API endpoints if temporarily disabled
6. Confirm SLOs are back within thresholds

**Post-Freeze Actions:**
1. Review any blocked operations attempted during freeze
2. Verify no data corruption occurred during incident
3. Confirm audit trail integrity (hash chain unbroken)

**Outputs:**
- Systems operational
- Data integrity verified
- Data freeze released
- SLOs nominal

---

### Phase 6 — Post-Incident Review

**Who:** Incident commander + Security Team + affected stakeholders

**Actions:**
1. Conduct blameless post-mortem within 5 business days
2. Document timeline of events
3. Assess response effectiveness:
   - Detection time (how long before detected?)
   - Response time (how long before containment?)
   - Communication (were stakeholders notified appropriately?)
4. Identify lessons learned
5. Update procedures, runbooks, and this plan based on findings
6. Update [threat model](./06-threat-model-v2.md) with new attack patterns
7. Schedule follow-up review in 30 days

**Outputs:**
- Post-incident report (blameless)
- Updated runbooks
- Updated threat model
- Updated incident response plan
- Updated [compliance checklist](../08-security/05-compliance-checklist.md) if control gaps found

---

## RACI Matrix

| Activity | Security Team | SRE | Engineering | Legal/Privacy | CISO |
|----------|---------------|-----|-------------|----------------|------|
| Detection | A | R | I | I | I |
| Triage | A | C | C | R | I |
| Containment | A | C | R | I | I |
| Eradication | A | C | R | I | I |
| Recovery | I | A | C | I | I |
| Post-Incident Review | A | C | C | C | R |

R = Responsible, A = Accountable, C = Consulted, I = Informed

---

## Communication Plan

### Internal Escalation

```
Detection → Security on-call → Security lead → CISO → Engineering VP
                                       ↓
                                  Legal (if breach)
```

### External Notification (if applicable)

| Audience | Trigger | Timeline | Owner |
|----------|----------|----------|-------|
| Affected tenants | Confirmed data breach | Within 72h (GDPR Art. 33) | Legal |
| Supervisory authority | Notifiable breach | Within 72h (GDPR Art. 33) | Legal/Privacy |
| Customers (if applicable) | Material breach | As required by contract | Legal |
| Public (if material) | Material breach | As required by regulation | Communications |

---

## Related Documents

- [11-incident-freeze.md](./11-incident-freeze.md) — data freeze procedures during investigation
- [10-forensic-bundle.md](./10-forensic-bundle.md) — forensic evidence collection
- [06-threat-model-v2.md](./06-threat-model-v2.md) — threat model (updated after incidents)
- [13-residual-risk-spec.md](./13-residual-risk-spec.md) — residual risk register
- [05-compliance-checklist.md](../08-security/05-compliance-checklist.md) — compliance control updates post-incident
- [05-runbooks.md](../09-operations/05-runbooks.md) — SRE runbooks (availability incidents)
- [07-authz-matrix.md](./07-authz-matrix.md) — access control matrix
