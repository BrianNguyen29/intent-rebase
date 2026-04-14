# 05 — Compliance Checklist

**Status:** Proposed  
**Phase:** Phase 3  
**Owner:** Security Team / Compliance Team

---

## Mục đích

Tracks compliance requirements against SOC 2 Type II, GDPR, and ISO 27001 control families. This checklist is a **bounded planning artifact** — items represent work-in-progress against defined controls, not a certification of compliance. Final compliance sign-off requires successful audit.

---

## SOC 2 Type II Controls

### CC1 — Control Environment

| Control ID | Control Description | Implementation Status | Evidence | Owner |
|------------|---------------------|----------------------|----------|-------|
| CC1.1 | Security awareness training program | In Progress | Training materials, completion tracking | Security Team |
| CC1.2 | Background checks on personnel | Planned | HR policy, screening procedures | HR |
| CC1.3 | Code of conduct and conflict of interest policy | Implemented | Employee handbook | HR |

### CC2 — Communication and Information

| Control ID | Control Description | Implementation Status | Evidence | Owner |
|------------|---------------------|----------------------|----------|-------|
| CC2.1 | Security objectives communicated to all staff | In Progress | Communications, policy acknowledgement | Security Team |
| CC2.2 | Incident reporting procedures communicated | Implemented | Incident response plan | Security Team |

### CC3 — Risk Assessment

| Control ID | Control Description | Implementation Status | Evidence | Owner |
|------------|---------------------|----------------------|----------|-------|
| CC3.1 | Annual risk assessment performed | In Progress | Risk assessment document | Security Team |
| CC3.2 | Threat model reviewed and updated | Implemented | [06-threat-model-v2.md](../14-governance/06-threat-model-v2.md) | Security Team |
| CC3.3 | Third-party risk assessment | Planned | Vendor assessment templates | Security Team |

### CC4 — Monitoring Activities

| Control ID | Control Description | Implementation Status | Evidence | Owner |
|------------|---------------------|----------------------|----------|-------|
| CC4.1 | Log aggregation and analysis | Implemented | Audit event spec, observability setup | SRE |
| CC4.2 | Anomaly detection and alerting | In Progress | Alerting rules, runbooks | SRE |
| CC4.3 | Periodic review of access rights | Planned | Access review procedure | Security Team |

### CC5 — Logical and Physical Access Controls

| Control ID | Control Description | Implementation Status | Evidence | Owner |
|------------|---------------------|----------------------|----------|-------|
| CC5.1 | Role-based access control (RBAC) implemented | Implemented | Authz matrix, tenant isolation | Security Team |
| CC5.2 | Multi-factor authentication for privileged users | Implemented | MFA on API key + JWT | Security Team |
| CC5.3 | Tenant isolation enforced at all layers | In Progress | [08-tenant-isolation.md](../14-governance/08-tenant-isolation.md) | Platform Team |
| CC5.4 | Data encryption in transit and at rest | Implemented | TLS, S3 encryption | Platform Team |

### CC6 — System Operations

| Control ID | Control Description | Implementation Status | Evidence | Owner |
|------------|---------------------|----------------------|----------|-------|
| CC6.1 | Change management procedure | Implemented | Phase gate checklist | Engineering |
| CC6.2 | Production deployments require approval | Implemented | Phase gate checklist | Engineering |
| CC6.3 | Incident data freeze procedures | Implemented | [11-incident-freeze.md](../14-governance/11-incident-freeze.md) | Security Team |

### CC7 — Change Management

| Control ID | Control Description | Implementation Status | Evidence | Owner |
|------------|---------------------|----------------------|----------|-------|
| CC7.1 | All changes documented and tracked | Implemented | PR history, audit trail | Engineering |
| CC7.2 | Rollback procedures documented | Implemented | Compensation action plan | Engineering |

### CC8 — Business Continuity

| Control ID | Control Description | Implementation Status | Evidence | Owner |
|------------|---------------------|----------------------|----------|-------|
| CC8.1 | SLOs defined and monitored | Implemented | [04-sre-and-slos.md](../09-operations/04-sre-and-slos.md) | SRE |
| CC8.2 | Runbooks for failure scenarios | In Progress | Runbook documentation | SRE |

---

## GDPR Compliance

### Article 5 — Principles of Processing

| Requirement | Implementation Status | Evidence | Owner |
|-------------|----------------------|----------|-------|
| Lawfulness, fairness, transparency | Implemented | Privacy policy, consent mechanisms | Legal |
| Purpose limitation | Implemented | Data handling spec | Engineering |
| Data minimization | Implemented | Field-level access control | Engineering |
| Accuracy | Implemented | Data validation rules | Engineering |
| Storage limitation | In Progress | Retention policy, deletion procedures | SRE |
| Integrity and confidentiality | Implemented | TLS, encryption, tenant isolation | Security Team |

### Article 17 — Right to Erasure

| Requirement | Implementation Status | Evidence | Owner |
|-------------|----------------------|----------|-------|
| Data deletion API | Planned | Deletion endpoint | Engineering |
| Deletion within SLA verification | Planned | Test evidence | SRE |
| S3 lifecycle policy enforcement | In Progress | S3 lifecycle rules | SRE |

### Article 30 — Records of Processing Activities

| Requirement | Implementation Status | Evidence | Owner |
|-------------|----------------------|----------|-------|
| Processing register maintained | In Progress | GDPR documentation | Legal |
| Data processing agreement template | Implemented | DPA template | Legal |

### Article 32 — Security of Processing

| Requirement | Implementation Status | Evidence | Owner |
|-------------|----------------------|----------|-------|
| Appropriate technical measures | In Progress | Encryption, access control, monitoring | Security Team |
| Incident notification procedure (72h) | Implemented | [11-incident-freeze.md](../14-governance/11-incident-freeze.md) | Security Team |

### Article 33 — Notification of Data Breach

| Requirement | Implementation Status | Evidence | Owner |
|-------------|----------------------|----------|-------|
| Breach detection and escalation | In Progress | Incident response plan | Security Team |
| Notification to supervisory authority within 72h | Planned | Notification procedure | Legal |

### Article 35 — Data Protection Impact Assessment

| Requirement | Implementation Status | Evidence | Owner |
|-------------|----------------------|----------|-------|
| DPIA for high-risk processing | Planned | DPIA template and process | Privacy Team |

---

## ISO 27001 Controls

### A.5 — Information Security Policies

| Control | Implementation Status | Evidence | Owner |
|---------|----------------------|----------|-------|
| A.5.1 Information security policy | Implemented | Security policy document | Security Team |
| A.5.2 Review of information security policy | Planned | Annual review procedure | Security Team |

### A.6 — Organization of Information Security

| Control | Implementation Status | Evidence | Owner |
|---------|----------------------|----------|-------|
| A.6.1 Internal organization | Implemented | Org chart, responsibilities | Security Team |
| A.6.2 Mobile devices and teleworking | In Progress | MDM policy | IT |
| A.6.3 Screen lock policy | Implemented | Endpoint security | IT |

### A.8 — Asset Management

| Control | Implementation Status | Evidence | Owner |
|---------|----------------------|----------|-------|
| A.8.1.1 Inventory of assets | Implemented | Asset inventory | Engineering |
| A.8.1.2 Asset ownership | Implemented | Ownership assignment | Engineering |
| A.8.2.1 Classification guidelines | Implemented | Data classification policy | Security Team |

### A.9 — Access Control

| Control | Implementation Status | Evidence | Owner |
|---------|----------------------|----------|-------|
| A.9.1.1 Access control policy | Implemented | [07-authz-matrix.md](../14-governance/07-authz-matrix.md) | Security Team |
| A.9.2.1 User registration | Implemented | User provisioning workflow | Engineering |
| A.9.2.2 Privilege management | Implemented | RBAC, least privilege | Security Team |
| A.9.4.1 Information access restriction | Implemented | Tenant isolation | Security Team |
| A.9.4.2 Secure log-on procedures | Implemented | JWT + API key + MFA | Security Team |

### A.10 — Cryptography

| Control | Implementation Status | Evidence | Owner |
|---------|----------------------|----------|-------|
| A.10.1.1 Cryptographic policy | Implemented | Encryption standards | Security Team |
| A.10.1.2 Key management | In Progress | Key rotation procedure | Engineering |

### A.12 — Operations Security

| Control | Implementation Status | Evidence | Owner |
|---------|----------------------|----------|-------|
| A.12.1.1 Documented operating procedures | Implemented | Runbooks, SLOs | SRE |
| A.12.2.1 Controls against malware | Implemented | Endpoint protection | IT |
| A.12.4.1 Event logging | Implemented | [01-audit-event-spec.md](../14-governance/01-audit-event-spec.md) | Security Team |
| A.12.4.2 Protection of logs | Implemented | Append-only, hash chain | Security Team |
| A.12.4.3 Administrator and operator logs | Implemented | Audit trail | Security Team |
| A.12.6.1 Technical vulnerability management | In Progress | Dependency scanning | Security Team |

### A.13 — Communications Security

| Control | Implementation Status | Evidence | Owner |
|---------|----------------------|----------|-------|
| A.13.1.1 Network control | Implemented | Network segmentation | Platform Team |
| A.13.2.1 Information transfer policy | Implemented | TLS in transit | Engineering |

### A.16 — Information Security Incident Management

| Control | Implementation Status | Evidence | Owner |
|---------|----------------------|----------|-------|
| A.16.1.1 Management responsibilities | Implemented | [14-incident-response-plan.md](../14-governance/14-incident-response-plan.md) | Security Team |
| A.16.1.2 Incident reporting | Implemented | Incident response plan | Security Team |
| A.16.1.3 Incident assessment | Implemented | Incident triage procedure | Security Team |
| A.16.1.4 Incident response | Implemented | Incident response plan | Security Team |
| A.16.1.5 Incident evidence retention | Implemented | [11-incident-freeze.md](../14-governance/11-incident-freeze.md) | Security Team |

### A.18 — Compliance

| Control | Implementation Status | Evidence | Owner |
|---------|----------------------|----------|-------|
| A.18.1.1 Identification of applicable legislation | Implemented | Legal review | Legal |
| A.18.1.2 Intellectual property rights | Implemented | IP policy | Legal |
| A.18.2.1 Independent audit | Planned | Audit procedure | Security Team |
| A.18.2.2 Independent review | Planned | Compliance review | Security Team |

---

## Related Documents

- [06-threat-model-v2.md](../14-governance/06-threat-model-v2.md) — threat model
- [13-residual-risk-spec.md](../14-governance/13-residual-risk-spec.md) — residual risk tracking
- [14-incident-response-plan.md](../14-governance/14-incident-response-plan.md) — incident response
- [11-incident-freeze.md](../14-governance/11-incident-freeze.md) — data freeze during investigation
- [04-sre-and-slos.md](../09-operations/04-sre-and-slos.md) — SLO definitions
