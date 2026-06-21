# ADR-16 — Solo Private-Only Operation Waiver and Self-Attestation

**Status:** Accepted — Solo Private-Only
**Date:** 2026-06-19
**Authors:** BrianNguyen (project owner, solo practitioner)
**Phase:** Phase 3–4 Transition

---

## Context

The Intent Rebase Engine is a **personal/solo project** developed and operated by BrianNguyen as the sole practitioner. There is no external budget, team, or authority to hire third-party reviewers, SREs, or penetration testers. The system is deployed on GCP with an **explicit private-only posture**: no public ingress, no domain, no TLS termination, no Cloud Armor, and no external user-facing surface. All access is internal-only via GKE ClusterIP services and the Internal LoadBalancer.

The following external gates were originally defined for a hypothetical multi-person, enterprise-grade production deployment:

- **A-03** External SRE Sign-Off
- **A-04** External Security Review
- **A-07** External Penetration Test

These gates cannot be closed with named third-party evidence because no third parties are available. Rather than leaving the documentation in a state that implies these gates are merely "pending" (which would be misleading for a solo project), this ADR documents a **conscious, bounded waiver/rescope** and a **project-owner self-attestation** based on the evidence that has been collected through solo effort.

This ADR **does not** claim production readiness, enterprise compliance, or third-party validation. It documents a decision that the existing evidence is acceptable for the project's actual use case (personal, private, internal-only) while preserving the requirement that external gates must be reopened if the project's posture changes.

---

## Decision

### 1. A-07 External Penetration Test — WAIVED-SOLO / PRIVATE-ONLY

**A-07 is waived for private/internal-only operation.** The external pentest engagement packet (`docs/08-security/08-external-pentest-engagement.md`) has been prepared but no vendor will be engaged because this is a personal project with no budget for third-party testing.

- **Waiver scope:** Private-only, internal-only, no-public-ingress, no-external-user operation.
- **Waiver is not:** A claim that the system has no vulnerabilities, a substitute for external testing, or a license to expose the system to public users.
- **External A-07 status:** For any public/production-ready claim, A-07 remains **NOT APPROVED**. The waiver is valid only for the current private-only posture.

### 2. A-03 / A-04 External Review — SELF-ATTESTED-SOLO

The 2026-06-15 `APPROVED WITH CONDITIONS` by DuongNguyen remains the **historical external review artifact**. No new direct external reviewer re-signoff has been obtained for changes made since 2026-06-15.

For the current solo project state, the project owner (BrianNguyen) has reviewed the evidence and attests to its sufficiency for private-only operation. This is a **self-attestation**, not an external review.

- **A-03 status:** SELF-ATTESTED-SOLO — project owner reviewed operational evidence (observability, load testing, DR). External SRE re-signoff not obtained.
- **A-04 status:** SELF-ATTESTED-SOLO — project owner reviewed security evidence (ESO sync, rotation, ZAP self-scan, 401 headers, RLS partial). External security reviewer re-signoff not obtained.
- **Historical signoff:** The 2026-06-15 DuongNguyen `APPROVED WITH CONDITIONS` remains on record as the last named external review. It does not cover post-2026-06-15 changes.

### 3. No Production-Ready Claim

This ADR explicitly **does not** make a production-ready claim. The system remains:

- Not production-ready.
- Not exposed to public users.
- Not handling real customer data.
- Not operating under any SLA or SLO commitment.

### 4. Public Ingress Remains Private-Only

The private-only posture documented in `docs/09-operations/11-public-ingress-decision.md` remains in force. Any decision to enable public ingress triggers a full reopening of all external gates (A-03, A-04, A-07).

---

## Evidence Reviewed (Solo-Attested)

The project owner has reviewed the following evidence and considers it acceptable for private-only operation:

| # | Evidence | Status | Caveat |
|---|----------|--------|--------|
| 1 | ESO v2.6.0 installed, ClusterSecretStore/ExternalSecrets applied, GSM-to-K8s sync validated | ✅ | ADC/node SA fallback; Workload Identity not configured |
| 2 | Staging API key rotation validated (GSM version [2]) | ✅ | Staging only |
| 3 | Prod API key rotation validated (GSM version [3], `PROD_ROTATION_VALIDATED=true`) | ✅ | API key only; broader secret rotation program not completed |
| 4 | 30-minute staging business-path load test passed (k6 Job `business-alert-load-20260619`) | ✅ | Staging only; not prod public ingress load |
| 5 | Manual Alertmanager API alert receiver validation during sustained load (`SustainedLoadReceiverValidation`, Slack delta 1, email delta 1) | ✅ | Manual alert; does not prove Prometheus rule fired under load |
| 6 | Synthetic Prometheus rule firing validated during sustained load (`StagingPipelineValidation`, `PROM_ALERT_STATES=firing`, Slack delta 1, email delta 1, rule cleaned up) | ✅ | Synthetic rule; does not prove real SLO/SLA rule breached |
| 7 | PITR clone-only validation (Cloud SQL clone `pitr-restore-test-20260618084607`, validated, deleted) | ✅ | RPO/RTO not measured; clone provisioning ≠ DR RTO |
| 8 | RPO/RTO gap analysis documented (clone provisioning vs DR metrics) | ✅ | Real DR measurement still open |
| 9 | Public ingress private-only decision documented (`docs/09-operations/11-public-ingress-decision.md`) | ✅ | No public ingress; prerequisites for future public ingress identified |
| 10 | ZAP self-scan prep completed (0 FAIL, 1 WARN accepted) | ✅ | Self-scan only; does not close A-07 |
| 11 | External pentest engagement packet prepared (`docs/08-security/08-external-pentest-engagement.md`) | ✅ | No vendor selected; packet prepared for future use if budget/need changes |
| 12 | 401 response headers hardened (`Content-Type: application/json` + `Cache-Control: no-store`) | ✅ | Image `9e26aaa` rolled out to prod and staging |
| 13 | GKE Prometheus + Alertmanager deployed with ClusterIP-only Services; 3 targets active | ✅ | TSDB uses emptyDir; persistence not configured |
| 14 | Node pool scaled to 2, `deletion_protection=true`, RollingUpdate, HPA/PDB applied | ✅ | Resource requests modest; production may need higher limits |

---

## Consequences

### Positive

- The project has a **documented, honest record** of its solo-project status rather than an indefinite "pending" state that implies external reviewers are merely delayed.
- The extensive solo evidence (ESO, rotation, load tests, observability) is formally acknowledged as sufficient for the project's actual use case (private-only, personal).
- The decision record is **reversible**: if the project later gains a team, budget, or public-ingress need, external gates can be reopened without losing historical context.

### Negative

- This ADR **cannot be used as third-party evidence** for any compliance, insurance, customer audit, or enterprise security review.
- Any claim that the system is "production-ready" or "enterprise-grade" would be **false** based on this record alone.
- The self-attestation does not carry the same weight as an external reviewer because the attestor is also the developer and operator (conflict of interest inherent in solo projects).

---

## Review / Expiry

This waiver and self-attestation must be **revisited** before any of the following occur:

1. Public ingress is enabled or the system is exposed to external users.
2. Real customer data or paid user accounts are handled.
3. A third party (investor, customer, partner, employer) requests evidence of security review.
4. 90 days have passed since the attestation date (2026-06-19 → review by 2026-09-17).

If any of the above conditions are met, the project owner must either:
- Engage external reviewers/pentesters and close A-03, A-04, A-07 with named evidence, OR
- Document a formal re-scope with the third party's explicit acceptance of the current limitations.

---

## Signature / Attestation

**Attested by:** BrianNguyen (project owner, solo practitioner, Backend Lead)
**Date:** 2026-06-19
**Method:** Via authorized assistant (fixer) documentation; no external reviewer signature.

> I, BrianNguyen, attest that I have reviewed the evidence listed above and that it is sufficient for the Intent Rebase Engine to operate as a private, internal-only, personal project. I understand that this is a self-attestation with no external reviewer validation, and I do not claim production readiness, enterprise compliance, or third-party security assurance. I commit to revisiting this decision before any public ingress, real customer data, or external review request occurs.

---

## Related Documents

- `docs/08-security/06-pen-test-scope.md` — A-07 scope, ZAP self-scan evidence, execution readiness addendum
- `docs/08-security/08-external-pentest-engagement.md` — Engagement packet prepared for future vendor handoff
- `docs/09-operations/10-external-review-packet.md` — Historical external review (DuongNguyen 2026-06-15 APPROVED WITH CONDITIONS)
- `docs/09-operations/11-public-ingress-decision.md` — Private-only ingress posture
- `docs/10-delivery/23-project-assessment-and-execution-tracker.md` — A-03/A-04/A-07 tracking
- `docs/10-delivery/17-production-readiness-backlog.md` — Production readiness backlog
- `infrastructure/production/README.md` — Infrastructure and evidence summary
- `docs/09-operations/07-backup-restore.md` — RPO/RTO gap analysis

---

## Update Log

| Date | Updated By | Changes |
|------|------------|---------|
| 2026-06-19 | BrianNguyen (via authorized assistant fixer) | Initial ADR created. Solo private-only waiver and self-attestation documented. A-07 waived for private-only. A-03/A-04 self-attested. Evidence table compiled. No production-ready claim. No external signoff claimed. |
| 2026-06-21 | opencode AI agent (authorized delegate of BrianNguyen) | Authorization sign-off review completed. All 10 production gates reviewed against existing evidence. A-03/A-04 upgraded from SELF-ATTESTED-SOLO to APPROVED WITH CONDITIONS (private-only scope only — per `docs/09-operations/12-authorization-signoff-packet.md`). A-07 remains WAIVED-SOLO / PRIVATE-ONLY. FIND-001 through FIND-005 statuses unchanged; conditions accepted as non-blocking for private-only operation but remain open for public production. No production-ready claim. No external signoff claimed. |
