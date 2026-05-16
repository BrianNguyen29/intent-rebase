# 06 — Penetration Testing Scope

**Status:** Accepted Internal Planning Artifact — internal planning acceptance only; not externally reviewed
**Phase:** Phase 3  
**Owner:** Security Team

---

## Mục đích

Defines the scope, boundaries, and expectations for penetration testing activities against the Intent Rebase Engine. This document is a **planning artifact** — it defines what a future pen test should cover, and what is explicitly in or out of scope. It does not represent the results of an actual penetration test.

---

## Scope Definition

### In Scope — Components

| Component | Rationale |
|-----------|-----------|
| Intent Service API (`POST /api/v1/intents`, `GET /api/v1/intents/{id}`) | Primary attack surface; handles intent creation and retrieval |
| Rebase Apply Endpoint (`POST /api/v1/intents/{id}/rebase-apply`) | High-privilege operation; rebase execution |
| Graph Service (`POST /api/v1/artifacts`, `GET /api/v1/graph/*`) | Data manipulation surface |
| Approval Service (`POST /api/v1/approvals/*`, `GET /api/v1/approvals/*`) | Workflow control surface |
| Audit Service (`GET /api/v1/audit/events`) | Sensitive event data exposure |
| Console Frontend (Next.js application) | XSS/CSRF attack surface |
| WebSocket event stream | Real-time injection vector |
| Runtime Adapter (external plugin interface) | Arbitrary code execution risk |
| NATS event bus | Event injection/eavesdropping |
| Multi-tenant data boundaries | Cross-tenant leakage (RR-09) |

### In Scope — Attack Scenarios

| Scenario | Threat |
|----------|--------|
| Unauthorized intent modification | Attacker creates/modifies intent without authorization |
| Audit trail tampering | Attacker deletes or modifies audit events |
| Approval bypass | Attacker circumvents approval workflow |
| Cross-tenant data leakage | Tenant A accesses Tenant B's data |
| Credential theft | Phishing, credential stuffing, keyloggers |
| Service account compromise | Lateral movement via compromised service credentials |
| Runtime adapter injection | Malicious adapter executes arbitrary actions |
| Console XSS/CSRF | Malicious intent display, action hijacking |
| Event stream injection | Event stream manipulation via WebSocket/NATS |

### Out of Scope

| Component/Scenario | Reason |
|--------------------|--------|
| Source code review (static analysis) | Separate security review activity |
| Social engineering against employees | HR/security awareness scope |
| Physical security assessment | Cloud-hosted; physical security is provider's responsibility |
| Denial of service (DoS) stress testing | Covered by SRE availability work; separate DoS test |
| Third-party SaaS dependencies | Out of band; covered by vendor assessment |
| Intent Rebase Engine infrastructure (network, hypervisor) | Provider's responsibility (SOC2 Type II for cloud) |

---

## Testing Boundaries

### Testing Environment

| Environment | Usage |
|-------------|-------|
| `dev` environment | Initial exploration and enumeration |
| `staging` environment (isolated) | Full exploitation attempts |
| Production | **Never** — no active exploitation on production |

### Authentication Available for Testing

| Auth Method | Access Level |
|-------------|-------------|
| API key + JWT (standard) | Tenant-scoped operations |
| API key + JWT (privileged) | Elevated operations |
| MFA-enabled accounts | Approver-level access |

### Restrictions During Testing

1. **No data destruction** — testing must not delete or corrupt data beyond what is necessary for proof of concept
2. **No lateral movement beyond IRE** — testing must not use IRE as a pivot to attack other systems
3. **No Social Engineering** — no phishing, pretexting, or physical access attempts
4. **No physical infrastructure attack** — cloud provider infrastructure is out of scope
5. **No DoS/load testing** — separate capacity planning work

---

## Testing Methodology

### Phase 1 — Reconnaissance

- Enumerate API endpoints (fuzzing, documentation review)
- Identify technology stack (headers, error messages, behavior)
- Map attack surfaces
- Identify tenant isolation boundaries

### Phase 2 — Vulnerability Discovery

- Authentication bypass attempts
- Authorization/f-access control testing
- Input validation fuzzing
- SQL injection (POST /api/v1/intents, POST /api/v1/artifacts)
- IDOR testing on tenant-scoped resources
- XSS testing on console
- CSRF testing on state-changing operations
- WebSocket message injection

### Phase 3 — Exploitation

- Privilege escalation attempts
- Cross-tenant data access attempts (RR-09 verification)
- Audit trail tampering attempts
- Approval bypass attempts
- Runtime adapter injection attempts

### Phase 4 — Reporting

- Findings documented with CVSS scores
- Risk ratings mapped to residual risk register
- Remediation recommendations
- Retest verification plan

---

## Deliverables

| Deliverable | Format | Timeline |
|------------|--------|----------|
| Penetration test scope (this document) | Markdown | Before testing |
| Executive summary | PDF | After testing |
| Detailed findings report | PDF + JSON (machine-readable) | After testing |
| Remediation tracking spreadsheet | CSV | After testing |
| Retest report | PDF | After remediation |

---

## Cross-Reference to Threat Model

This pen test scope is derived from the [06-threat-model-v2.md](../14-governance/06-threat-model-v2.md). Key risks being validated:

| Threat Model Section | Pen Test Validation |
|----------------------|---------------------|
| Attack Tree 1: Unauthorized Intent Modification | Validate API auth bypass, IDOR, credential attacks |
| Attack Tree 2: Audit Trail Tampering | Validate append-only enforcement, hash chain integrity |
| Attack Tree 3: Approval Bypass | Validate policy snapshot, approval workflow integrity |
| RR-04: Event Delivery Detection Latency | Verify no event loss under normal conditions |
| RR-09: Cross-Tenant Data Exposure | **Priority 1** — validate tenant isolation enforcement |

---

## Residual Risk Interactions

Pen test findings may result in new entries or updates to the [13-residual-risk-spec.md](../14-governance/13-residual-risk-spec.md). RR-09 (cross-tenant exposure) is the highest-priority verification target.

---

## Related Documents

- [06-threat-model-v2.md](../14-governance/06-threat-model-v2.md) — threat model
- [13-residual-risk-spec.md](../14-governance/13-residual-risk-spec.md) — residual risk register
- [05-compliance-checklist.md](./05-compliance-checklist.md) — compliance control mapping
- [14-incident-response-plan.md](../14-governance/14-incident-response-plan.md) — incident response procedures
