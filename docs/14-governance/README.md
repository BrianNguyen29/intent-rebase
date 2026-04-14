# Audit & Governance Pack

## Mục đích

Bộ tài liệu này định nghĩa các tiêu chuẩn về **audit, governance, compliance, và security** cho Intent Rebase Engine. Nó bao gồm các specs và guidelines mà đội security, compliance, và SRE cần để vận hành hệ thống ở mức production.

---

## Chỉ mục Tài liệu

| ID | Tiêu đề | Phase | Mục đích |
|----|---------|-------|---------|
| [01](./01-audit-event-spec.md) | Audit Event Specification | P1+ | Canonical event schema cho audit trail |
| [02](./02-provenance-spec.md) | Provenance Specification | P1+ | Artifact provenance chain và verification |
| [03](./03-policy-snapshot-spec.md) | Policy Snapshot Specification | P1+ | Immutable policy snapshots cho compliance |
| [04](./04-approval-revalidation.md) | Approval Scope & Revalidation | P1+ | Approval invalidation và revalidation specs |
| [05](./05-immutable-retention-tamper-resistance.md) | Immutable Retention & Tamper Resistance | P2+ | Data immutability và tamper detection |
| [06](./06-threat-model-v2.md) | Threat Model v2 | P3 | Comprehensive threat model update |
| [07](./07-authz-matrix.md) | Authorization Matrix | P1+ | Role-based access control matrix |
| [08](./08-tenant-isolation.md) | Tenant Isolation | P3 | Tenant data isolation guarantees |
| [09](./09-data-handling-redaction.md) | Data Handling & Redaction | P1+ | PII handling, redaction, privacy |
| [10](./10-forensic-bundle.md) | Forensic Bundle | P3 | Forensic bundle structure và replay |
| [11](./11-incident-freeze.md) | Incident Freeze | P2+ | Data freeze during incident investigation |
| [12](./12-replay-compatibility.md) | Replay Compatibility | P2+ | Replay guarantees và compatibility |
| [13](./13-residual-risk-spec.md) | Residual Risk Specification | P3 | Risk acceptance và residual risk tracking |
| [14](./14-incident-response-plan.md) | Incident Response Plan | P2+ | End-to-end incident response process |

---

## Liên kết nội bộ

- **ADR Pack:** `../13-adrs/README.md` — architectural decisions driving governance requirements
- **Security Docs:** `../08-security/` — threat model, authn/authz, privacy
- **Operations Docs:** `../09-operations/` — observability, SLOs, runbooks
- **Agent Guide:** `../12-agents/01-agent-implementation-guide.md` — agent behavior requirements

---

## Quy tắc chung

1. **Audit trail is append-only** — no UPDATE/DELETE on audit tables
2. **All governance data is tenant-scoped** — no cross-tenant visibility
3. **Tamper detection must be active** — any tampering attempt logged and alerted
4. **Compliance evidence must be exportable** — SIEM, PDF reports, raw data export

---

## Ownership

| Area | Owner | Review Cadence |
|------|-------|----------------|
| Audit events | Security Team | Quarterly |
| Policy snapshots | Compliance Team | Annual |
| Authorization matrix | Security Team | On role change |
| Tenant isolation | Platform Team | Quarterly |
| Forensic bundles | Security Team | Per incident |
| Threat model | Security Team | Annual |

---

## Compliance Targets

| Standard | Applicability | Status |
|----------|--------------|--------|
| SOC 2 Type II | All tenants | Planned (Phase 4) |
| GDPR | EU tenants | Planned (Phase 4) |
| HIPAA | Healthcare tenants | Future |
| ISO 27001 | All tenants | Future |