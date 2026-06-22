# Reviewer Intake Package — A-03 / A-04 / A-07 Evidence

> **Status:** Intake package prepared 2026-06-22. No external reviewer engaged. No sign-off claimed. This is an evidence index for future review.
> **Scope:** Private-only solo operation. All evidence is bounded to internal/staging environments. No public ingress.

---

## 1. Purpose

This document indexes the evidence collected for three external gates (A-03 SRE, A-04 Security, A-07 Penetration Test) so that a future external reviewer can efficiently locate evidence and assess gaps.

It is **not a completed review** and does **not** contain any reviewer sign-off. All reviewer fields are TBD.

---

## 2. Evidence Index by Gate

### A-03 — External SRE Operational Review

| Evidence Category | Document | Status | Key Claim |
|---|---|---|---|
| Infrastructure scaffold | `infrastructure/production/README.md` §Applied Scaffold Status | GCP core applied | VPC, subnet, GKE zonal cluster, Cloud SQL, GCS bucket, artifact registry |
| App deployment | `infrastructure/production/README.md` §Applied App Smoke Status | Internal smoke deploy running | Pod running, health/ready 200, JWT guard passed, SQL-backed router initialized |
| Prometheus + Alertmanager | `infrastructure/production/README.md` §Phase 2 Observability | Deployed on GKE, 4 targets UP | Prometheus, Alertmanager, intent-api, NATS targets all UP |
| Prometheus persistent storage | `infrastructure/production/README.md` §Prometheus PVC Persistence | PVC applied 2026-06-22 | `prometheus-storage` 10Gi RWO, TSDB WAL replay started, `fsGroup: 65534` fix |
| SLO rules + k6 validation | `docs/09-operations/11-slo-targets.md` §5.6–5.8 | Real app SLO rules applied 2026-06-22; validated at 5 VU, 20 VU, and 50 VU | `IntentApiLatencyP95High`, `IntentApiErrorRateHigh` loaded and `inactive` under normal load; temporary validation rule fired and removed; 5 VU k6 passed (1830 iterations, p95=687µs, 0% errors); 20 VU passed (7219 iterations, p95=760µs, 0% 5xx); 50 VU passed (17989 iterations, p95=127µs, 0% 5xx) |
| Load testing | `infrastructure/production/README.md` §Load Test Results | 7-min bounded k6 passed 2026-06-21 and 2026-06-22 | 100% checks, 0% failures, p95 < 1ms, 4.35 req/s |
| Staging 30-min business-path load | `infrastructure/production/README.md` §30-Minute Staging Business-Path Load | 8930 iterations, 0% failure, p95 17.5ms | Manual Alertmanager alert posted during sustained load; Slack/email delta 1 each |
| Receiver validation | `infrastructure/production/README.md` §Alert Validation | Slack + SMTP direct transport + Alertmanager POST validated | Real credentials out-of-band; temporary container + GKE retest passed |
| SLO targets document | `docs/09-operations/11-slo-targets.md` | Active 2026-06-22 | SLO definitions, error budgets (conceptual), measurement methods, blockers, forbidden claims |
| Secrets management | `infrastructure/production/README.md` §Phase 3 Security | GSM + ESO v2.6.0 + ClusterSecretStore + ExternalSecrets applied | 22 secrets prod/staging; staging API key rotation validated (v2); prod API key rotation validated (v3) |
| JWT rotation | `docs/09-operations/05-runbooks.md` RB21 Evidence | Dual-key rotation validated 2026-06-21; grace window closed 2026-06-22 | GSM v2, ESO sync, deployment restart, health 200 with new+old tokens; `JWT_SECRET_PREVIOUS` removed from K8s secret, new token verified, old token no longer in runtime |
| DB URL rotation | `docs/09-operations/05-runbooks.md` | Cloud SQL user password rotated 2026-06-21 | GSM v2, ESO sync, deployment restart, health 200, DB_CONNECTION_OK |
| DR / PITR | `docs/09-operations/07-backup-restore.md` | Clone-only validated 2026-06-18; formal RTO drill 2026-06-22 | PITR clone created, validated, deleted; formal RTO ~21m37s (clone 16m58s + app 6s); temp resources cleaned up |
| RPO measurement | `docs/09-operations/07-backup-restore.md` §RPO Measurement | Empirically measured 2026-06-22 | PITR enabled, automated backups daily 03:00, WAL archiving enabled, transaction log retention 7 days; theoretical RPO < 1 min; **empirical RPO ~2.7 seconds** measured via live marker→WAL archive lag + PITR clone verification |
| Runbooks | `docs/09-operations/05-runbooks.md` RB1–RB14 | Documented | Prometheus query, restart procedure, NATS consumer lag, checkpoint rollback, audit failover, compensation retry, rebase timeout, approval escalation, quarantine, DLQ investigation, secret rotation, synthetic data seeding |
| NATS pilot | `infrastructure/production/README.md` §NATS JetStream Pilot | Provisioned and validated 2026-06-21 | StatefulSet, PVC, Service, ConfigMap, validation Job; stream create/list, durable consumer create/list, pub/consume verified |
| NATS Prometheus scrape | `infrastructure/production/README.md` §NATS Prometheus Scrape Target | Applied 2026-06-22 | `prometheus-nats-exporter:0.15.0` sidecar on port 7777; Prometheus target UP; `gnatsd_connz_num_connections` queryable |
| NATS app consumer | `infrastructure/production/README.md` §NATS App Consumer Wired | Wired 2026-06-22 | `NATS_URL=nats://nats:4222`, `NATS_TOKEN` token auth enforced, `INTENT_API_NATS_CONSUMER=true`, `INTENT_API_NATS_FULL_CONSUMER=true`, `INTENT_API_NATS_DLQ_WORKER=true`, `INTENT_API_NATS_DLQ_REPLAY_WORKER=true`; `audit_events` stream created; three consumers (checkpoint_creator, snapshot_creator, notifier) polling; DLQ metrics/replay workers started; not production HA cluster, not per-tenant streams, not production-certified |
| Forensic bucket | `infrastructure/production/README.md` §Dedicated Forensic Immutable Bucket | Created 2026-06-22 | `gs://forensic-evidence-ferrum-497801-ed2c5bdd`; versioning, 30-day retention (UNLOCKED), PAP enforced, uniform access; **GCS BundleStorage backend implemented 2026-06-22** using metadata-server OAuth (no HMAC keys); end-to-end smoke test passed (upload/download/delete verified, 30-day retention active); not S3 Object Lock compliant |
| CI/CD audit trail | `infrastructure/production/README.md` §Lane 6 CI/CD Audit Trail | `.github/workflows/audit-trail.yml` created 2026-06-22 | Manual workflow_dispatch only; quality + SBOM jobs; signing deferred; no auto-deploy |

**A-03 Blockers for Public Production:**
- FIND-001: Real SLO rules validated under 5 VU, 20 VU, and 50 VU bounded internal load; SLO breach firing (artificial latency/error injection) not tested; error budgets not committed; node-exporter/kube-state-metrics not deployed
- FIND-004: Formal RPO empirically measured (~2.7s WAL lag); formal RTO with live-traffic cutover; scheduled DR drills remain open
- Public ingress: Not enabled; prerequisite checklist exists
- No external SRE has reviewed or signed off unconditionally

**A-03 Status:** `APPROVED WITH CONDITIONS — PRIVATE-ONLY` (self-attested by project owner; external re-signoff NOT obtained)

---

### A-04 — External Security Architecture Review

| Evidence Category | Document | Status | Key Claim |
|---|---|---|---|
| 401 header hardening | `infrastructure/production/README.md` §401 Response Header Hardening | Applied in image `9e26aaa` | `Content-Type: application/json` + `Cache-Control: no-store` on all 401 paths; ZAP re-run confirmed `Content-Type Header Missing` → PASS |
| ZAP self-scan | `infrastructure/production/README.md` §ZAP Baseline Self-Scan | 0 FAIL, 1 WARN accepted | Unauthenticated: 0 FAIL, 1 WARN (`Non-Storable Content` on 401 — expected); authenticated attempts blocked by tool import limitation |
| JWT dual-key | `docs/09-operations/05-runbooks.md` RB21 | Implemented and validated 2026-06-21; grace window closed 2026-06-22 | `JWT_SECRET` + `JWT_SECRET_PREVIOUS` support; rotation exercised; old secret removed from runtime |
| API key rotation | `infrastructure/production/README.md` §Phase 3 Security | Staging v2 + prod v3 validated | GSM → ESO → K8s Secret → deployment restart → smoke; hash match without printing secrets |
| RLS wrapping | `docs/10-delivery/17-production-readiness-backlog.md` P1-S5i | Bounded partial (13 RLC tests pass) | Handler-level tenant guards in all scoped forensic/orchestration/artifact/replay handlers; `OptionalRlsTenantClaims` + mismatch rejection + `begin_with_tenant` |
| Threat model v2 | `docs/14-governance/06-threat-model-v2.md` | Accepted internal planning artifact | 5 threats, 5 mitigations; no external security review |
| Pen test scope | `docs/08-security/06-pen-test-scope.md` | Accepted internal planning artifact | 10 in-scope components, 9 attack scenarios, 6 out-of-scope exclusions |
| Engagement packet | `docs/08-security/08-external-pentest-engagement.md` | Prepared for vendor handoff | Staging env, synthetic data seeded, access options (VPN/IAP/bastion/temporary ingress), rules of engagement, report format requirements |
| Secrets inventory | `docs/09-operations/08-secrets-inventory.md` | 22 secrets tracked | Inventory, rotation procedures, per-secret status; JWT/API key/DB URL rotations validated; NATS/S3 pending provisioning |
| Authn/AuthZ | `docs/08-security/02-authn-authz.md` | Documented | JWT production guard, RLS context helper, RLS-aware pool, `create_graph_node` RLS wrapping |

**A-04 Blockers for Public Production:**
- FIND-002: Full RLS transaction wrapping across all SQL paths; NATS per-tenant streams (ADR-15); production certification
- FIND-003: Broader secret rotation (JWT grace window closed, DB URL rotated, NATS token provisioned and rotated; S3 not wired to app); GCS forensic backend uses metadata-server OAuth (no HMAC keys)
- A-07: External pen test not executed; ZAP self-scan is prep only
- No external security reviewer has reviewed or signed off unconditionally

**A-04 Status:** `APPROVED WITH CONDITIONS — PRIVATE-ONLY` (historical DuongNguyen 2026-06-15; external re-signoff NOT obtained for post-2026-06-15 changes)

---

### A-07 — External Penetration Test

| Evidence Category | Document | Status | Key Claim |
|---|---|---|---|
| Staging environment | `infrastructure/production/README.md` §A-07 Staging Environment | Deployed 2026-06-18 | Namespace `intent-rebase-staging`, clone DB, K8s Secret with staging credentials, Deployment + Service, image `intent-api:9e26aaa` |
| Synthetic data | `docs/08-security/08-external-pentest-engagement.md` §5.2 | Seeded 2026-06-19 | 2 tenants, 5 intents, 5 version-2s, 2 graph nodes, 1 webhook subscription; approval/audit/runtime-adapter mocks not seeded |
| ZAP self-scan | `infrastructure/production/README.md` §ZAP Baseline Self-Scan | Prep only | 0 FAIL, 1 WARN accepted; authenticated scan blocked by tool limitation |
| Engagement packet | `docs/08-security/08-external-pentest-engagement.md` | Ready for vendor | Scope, ROE, report format, remediation/retest process, signoff criteria, pre/post checklists |

**A-07 Blockers:**
- No external vendor/tester engaged
- No NDA/SOW signed
- No test window scheduled
- Access method (VPN/IAP/bastion/ingress) not decided
- No report received
- No findings remediated or retested

**A-07 Status:** `WAIVED-SOLO / PRIVATE-ONLY — NOT APPROVED` (no external pen test executed; no production-ready claim)

---

## 3. Access Posture for External Reviewers

| Item | Current State | Notes |
|------|---------------|-------|
| Public ingress | **None** | ClusterIP only; Internal LoadBalancer `10.0.0.12` (private) |
| TLS | **Not deployed** | Required before public ingress; no certificates |
| WAF / Cloud Armor | **Not deployed** | Required before public ingress |
| Domain | **None** | Required before public ingress |
| NATS | **Single-node pilot** | Token auth, no TLS, no HA; internal ClusterIP only |
| Forensic storage | **GCS BundleStorage backend implemented** | GCS bucket `gs://forensic-evidence-ferrum-497801-ed2c5bdd`; metadata-server OAuth (no HMAC keys); runtime wired; end-to-end smoke test passed; `roles/storage.objectAdmin` at bucket level (broader than least-privilege); retention policy UNLOCKED; no Bucket Lock; not S3 Object Lock compliant |
| Database | **Cloud SQL private IP** | PITR enabled, backups daily, automated user rotation possible |
| Secrets | **GSM + ESO** | 22 secrets; API key/JWT/DB URL/NATS token rotations validated; S3 not wired |

---

## 4. Package Contents (Quick Reference)

| Document | Path | Purpose |
|----------|------|---------|
| Infrastructure Evidence | `infrastructure/production/README.md` | Canonical record of all applied GCP/GKE resources, tests, and validation results |
| SLO Targets | `docs/09-operations/11-slo-targets.md` | Formal SLO definitions, validation evidence, blockers, forbidden claims |
| Runbooks | `docs/09-operations/05-runbooks.md` | RB1–RB14 operational procedures with evidence rows |
| Backup/Restore | `docs/09-operations/07-backup-restore.md` | PITR clone validation, DR drill evidence, RPO measurement |
| Secrets Inventory | `docs/09-operations/08-secrets-inventory.md` | 22 secrets, rotation status, per-secret evidence |
| External Review Packet | `docs/09-operations/10-external-review-packet.md` | Historical DuongNguyen review (2026-06-15), findings tracker (FIND-001–FIND-005), sign-off |
| Auth Sign-off Packet | `docs/09-operations/12-authorization-signoff-packet.md` | Project owner self-attestation of all 10 gates (2026-06-21) |
| Pen Test Engagement | `docs/08-security/08-external-pentest-engagement.md` | Vendor engagement packet (scope, ROE, credentials policy, synthetic data, report format) |
| Pen Test Scope | `docs/08-security/06-pen-test-scope.md` | 10 in-scope components, 9 attack scenarios, 6 out-of-scope exclusions |
| Threat Model v2 | `docs/14-governance/06-threat-model-v2.md` | 5 threats, 5 mitigations, residual risk assessment |
| Authn/Authz | `docs/08-security/02-authn-authz.md` | Authentication & authorization architecture |
| Execution Plan | `docs/10-delivery/26-post-signoff-execution-plan.md` | 20 lanes, Phase A–D, forbidden claims table |
| Tracker | `docs/10-delivery/23-project-assessment-and-execution-tracker.md` | Dated execution log with evidence rows |
| Solo Waiver | `docs/13-adrs/16-solo-private-operation-waiver.md` | ADR-16 terms for private-only solo operation |
| Public Ingress Decision | `docs/09-operations/11-public-ingress-decision.md` | Decision to stay private-only; prerequisite checklist for future public ingress |

---

## 5. Forbidden Claims (Enforced)

| Forbidden Claim | Actual Status |
|---------------|-------------|
| External SRE sign-off obtained | `NOT OBTAINED` — historical conditional sign-off (DuongNguyen 2026-06-15) on record; post-2026-06-15 changes not re-reviewed by external SRE |
| External security review complete | `NOT OBTAINED` — historical conditional sign-off (DuongNguyen 2026-06-15) on record; post-2026-06-15 changes not re-reviewed by external security reviewer |
| A-07 passed / external pen test executed | `NOT APPROVED` — ZAP self-scan prep only; no external vendor engaged; no report received |
| Production-ready | `NOT CLAIMED` — private-only solo operation with open gates; all public production prerequisites tracked as open lanes |
| Public ingress load tested | `NOT CLAIMED` — internal ClusterIP only; no public ingress, no TLS, no CDN, no edge load |
| SLO committed with penalties | `NOT CLAIMED` — conceptual error budgets only; no contractual SLA; no committed penalties |
| DR validated with live traffic | `NOT CLAIMED` — clone-only validation; no live-traffic cutover; RPO not empirically measured against live writes |
| Forensic storage production-ready | `NOT CLAIMED` — bucket exists with versioning/retention/PAP; GCS BundleStorage backend implemented and smoke-tested; `roles/storage.objectAdmin` broader than least-privilege; Bucket Lock deferred; not S3 Object Lock compliant; forensic bundle download API not production-validated for GCS backend |
| NATS production-ready | `NOT CLAIMED` — single-node pilot, token auth but no TLS/mTLS/HA, no per-tenant streams, DLQ stream not yet created, not production-certified |
| Commercial-ready / Enterprise-ready | `NOT CLAIMED` — private-only; requires SOC2/GDPR/team/SLA/SBOM/IR drills/customer docs |

---

## 6. Update Log

| Date | Updated By | Changes |
|------|------------|---------|
| 2026-06-22 | BrianNguyen (via authorized assistant fixer) | Initial intake package created: evidence index for A-03/A-04/A-07, access posture, quick reference, forbidden claims. No external reviewer engaged. No sign-off claimed. All statuses reflect private-only solo operation with open gates. |
| 2026-06-22 | BrianNguyen (via authorized assistant fixer) | Evidence refreshed: SLO validation updated to include 20 VU and 50 VU load test results; RPO updated to empirical ~2.7s measurement; A-03 blockers updated (FIND-001 partial, FIND-004 RPO measured); A-04 blockers updated (NATS token provisioned); access posture updated (forensic storage runtime wired, secrets NATS token validated). No external reviewer engaged. No sign-off claimed. |
