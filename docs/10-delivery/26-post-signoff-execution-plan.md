# Post-Signoff Execution Plan

> **Status:** POST-SIGNOFF PLANNING — derived from `12-authorization-signoff-packet.md` (2026-06-21)
> **Date:** 2026-06-21
> **Owner:** BrianNguyen (Backend Lead, solo practitioner)
> **Scope:** Concrete next steps after private-only solo close-out. No public production-ready claim. No external signoff claim. No committed external pen test report.
> **Non-Production Caveat:** This plan is for a solo-operated, private-only system. All public-production, commercial, and enterprise readiness gates remain open and require external third-party evidence before any public claims.

---

## 1. Status Snapshot After Sign-off

**Private-only solo close-out is complete (commit `845b8b1`).** The system has extensive evidence for private-only operation, but **is not production-ready** for public or commercial use.

| Gate | Sign-off Status | Scope | Key Evidence |
|------|----------------|-------|-------------|
| A-07 External Pentest | 🚫 WAIVED-SOLO / PRIVATE-ONLY | Private-only only | ZAP self-scan prep (0 FAIL, 1 WARN); engagement packet ready; no vendor engaged |
| A-03 External SRE Re-Signoff | 🟡 APPROVED WITH CONDITIONS | Private-only solo | GKE Prometheus + Alertmanager deployed, 30-min staging load passed, receiver validated, PITR clone-only DR smoke passed, API key rotation validated |
| A-04 External Security Re-Signoff | 🟡 APPROVED WITH CONDITIONS | Private-only solo | 401 headers hardened, ZAP verified, GSM + ESO deployed, API key rotation validated, JWT dual-key implemented, bounded RLS delivered (13 RLC tests), chain-hash (ADR-14) |
| Public Ingress / TLS / WAF | ✅ APPROVED — PRIVATE-ONLY POSTURE | Private-only | Internal LB (`10.0.0.12`) + ClusterIP only; no public domain, no TLS, no Cloud Armor |
| NATS JetStream Topology | 📋 DESIGN APPROVED — IMPLEMENTATION PENDING | Planning | ADR-15 staged migration design complete; local consumer gates delivered; no GCP NATS deployed |
| S3 / GCS Forensic Storage | 📋 DESIGN APPROVED — IMPLEMENTATION PENDING | Planning | Chain-hash (ADR-14) + `S3BundleStorage` seam delivered; GCS bucket with retention exists; NOT Object Lock; no production validation |
| Broader Secret Rotation | 🟡 PARTIALLY APPROVED | Private-only | API key rotation validated (prod + staging); JWT dual-key ready; DB URL pending coordination; NATS/S3 gated |
| Production Load / SLO | 🟡 APPROVED WITH CONDITIONS | Staging only | 30-min staging business-path load passed (8934 requests, 0% failure, p95 17.5ms); receiver + synthetic rule validated; no formal SLO doc; no prod load |
| Full DR Maturity | 🟡 APPROVED WITH CONDITIONS | Solo smoke | PITR clone validated (ready ~20m); DR smoke (clone+app+health+auth 201/200); no formal RPO/RTO measurement; no live cutover |
| CI / Audit Trail | ✅ APPROVED — LOCAL GATES | Solo | Local `verify-fast` + `smoke.yml` PR gating; no remote CI; manual image builds; no SBOM/signing |

**Reopening triggers (per ADR-16 §Review/Expiry):**
1. Public ingress enabled or system exposed to external users
2. Real customer data or paid accounts handled
3. Third party (investor, customer, partner, employer) requests security evidence
4. 2026-09-17 review deadline reached (90 days from 2026-06-19)

---

## 2. External Artifact Intake Checklist

### A-07 — External Penetration Test (NOT APPROVED — OPEN)

A-07 remains **OPEN** until all of the following are satisfied. The engagement packet (`docs/08-security/08-external-pentest-engagement.md`) is ready for vendor handoff.

| # | Required Artifact | Status | Owner | Notes |
|---|------------------|--------|-------|-------|
| 1 | Vendor/tester selected and contact details recorded | 🔴 TODO | Security | Engagement packet §2 has placeholder `TODO` |
| 2 | NDA / SOW / ROE signed with vendor | 🔴 TODO | Security / Legal | Use packet §6 rules of engagement as ROE baseline |
| 3 | Access path selected (VPN / IAP / bastion / temporary ingress) and tested | 🔴 TODO | SRE | Packet §4.2 options A–D; recommend Option A (VPN) or B (IAP) |
| 4 | Synthetic data seeded in staging | ✅ DONE | Backend Lead | 2 tenants, 5 intents, 5 version-2s, 2 graph nodes, 1 webhook subscription |
| 5 | Credentials generated and delivered out-of-band | 🔴 TODO | Security | Staging-only API key + JWT signing token; no DB credentials |
| 6 | Logging and monitoring enabled for staging | 🔴 TODO | SRE | Prometheus + Alertmanager already on GKE prod; staging telemetry gap |
| 7 | Stop-contact channel established | 🔴 TODO | Backend Lead | Slack, email, phone |
| 8 | Teardown plan documented (rotation, access removal, cleanup) | 🔴 TODO | SRE | Post-engagement checklist in packet §12 |
| 9 | Tester onboarding completed (scope, rules, access walkthrough) | 🔴 TODO | Security | Use packet §3 scope + §6 rules |
| 10 | Legal/contractual scope agreement signed | 🔴 TODO | Security / Legal | No production exploitation, no data destruction, no lateral movement |
| 11 | External report received (PDF executive + PDF technical + JSON/CSV) | 🔴 TODO | External Tester | Must include: IDs, titles, severity, CVSS, CWE, OWASP API mapping, reproduction steps, evidence, impact, remediation, retest status |
| 12 | No open CRITICAL findings | 🔴 TODO | Backend Lead | If any CRITICAL: fix, retest, confirm |
| 13 | No open HIGH findings unless formally risk-accepted in writing | 🔴 TODO | Backend Lead | If any HIGH: fix, retest, confirm; or risk-accept with compensating controls |
| 14 | Material MEDIUM findings remediated or risk-accepted | 🔴 TODO | Backend Lead | 60-day plan required |
| 15 | Retest passed for all fixed CRITICAL + HIGH | 🔴 TODO | External Tester | Vendor retest confirmation (short PDF or signed statement) |
| 16 | All tester credentials rotated; temporary access removed | 🔴 TODO | SRE | VPN/IAP/bastion/ingress teardown |
| 17 | Staging environment verified intact (no leftover tester data/accounts/scripts) | 🔴 TODO | SRE | Post-engagement checklist item 8 |
| 18 | Final signoff recorded in `docs/09-operations/10-external-review-packet.md` | 🔴 TODO | Security / Backend Lead | Update tracker and A-07 status line |
| 19 | Update log entry added to engagement packet + external review packet | 🔴 TODO | Backend Lead | Evidence of closure |

**A-07 sign-off rule:** A-07 status may be updated from `NOT APPROVED` to `APPROVED WITH CONDITIONS` only after items 1–19 are complete. It may be updated to `APPROVED` only after items 1–19 are complete AND no open CRITICAL/HIGH findings remain AND all conditions are resolved.

---

### A-03 — External SRE Re-Signoff

Historical DuongNguyen `APPROVED WITH CONDITIONS` (2026-06-15) remains on record. This sign-off was **upgraded** to `APPROVED WITH CONDITIONS — PRIVATE-ONLY SOLO OPERATION` in the 2026-06-21 authorization packet. **For any public production claim, A-03 must be reopened with named external SRE evidence.**

| # | Required Artifact | Status | Owner | Notes |
|---|------------------|--------|-------|-------|
| 1 | FIND-001 closed with measured evidence (real SLO rule breach under sustained load) | 🔴 OPEN | SRE | Synthetic rule validated; real SLO/SLA rule breach not tested. See Lane 4 below. |
| 2 | FIND-004 closed with measured evidence (RPO/RTO measured against live traffic) | 🔴 OPEN | SRE | Clone-only validated; formal RPO/RTO drill pending. See Lane 5 below. |
| 3 | Named external SRE reviewer signs Section H of external review packet | 🔴 TODO | External SRE | Cannot be self-signed; must be independent third party |
| 4 | Production telemetry persistence (TSDB not emptyDir) | 🔴 TODO | SRE | Current Prometheus uses emptyDir; lost on pod restart |
| 5 | Full production NATS/JetStream topology deployed and validated | 🔴 TODO | SRE | See Lane 2 below |
| 6 | Full production S3/GCS forensic storage validated | 🔴 TODO | SRE | See Lane 3 below |
| 7 | Production load test (L5) with public ingress | 🔴 TODO | SRE | See Lane 4 below; gated on public ingress |
| 8 | Update log entry in external review packet + tracker | 🔴 TODO | Backend Lead | Evidence of unconditional signoff |

---

### A-04 — External Security Re-Signoff

Historical DuongNguyen `APPROVED WITH CONDITIONS` (2026-06-15) remains on record. Upgraded to `APPROVED WITH CONDITIONS — PRIVATE-ONLY SOLO OPERATION` in 2026-06-21 packet. **For any public production claim, A-04 must be reopened with named external security reviewer evidence.**

| # | Required Artifact | Status | Owner | Notes |
|---|------------------|--------|-------|-------|
| 1 | FIND-002 closed (full production RLS certification across all SQL paths + NATS topology) | 🔴 OPEN | Security | Bounded RLS delivered (13 RLC tests); production certification pending. See Lane 2 (NATS) + A-03 SRE coordination. |
| 2 | FIND-003 closed (broader secret rotation exercised: DB URL, JWT, NATS, S3) | 🔴 OPEN | Security | API key done; JWT ready; DB URL pending; NATS/S3 gated. See Lane 1. |
| 3 | FIND-005 closed (A-07 pen test executed and remediated) | 🔴 OPEN | Security | A-07 is separate gate; must close first. |
| 4 | Named external security reviewer signs Section H of external review packet | 🔴 TODO | External Security | Cannot be self-signed; must be independent third party |
| 5 | Threat model v2 validated against production surface | 🔴 TODO | Security | Current threat model is internal planning artifact |
| 6 | Update log entry in external review packet + tracker | 🔴 TODO | Backend Lead | Evidence of unconditional signoff |

---

## 3. Gated Public Path — Prerequisites Before Public Ingress

**Current decision:** Stay private-only. No public ingress, domain, TLS, or Cloud Armor.

**If business need arises, the following must be completed IN ORDER before enabling any public surface:**

1. **A-04 updated security signoff** — external security reviewer reviews the new external surface (TLS, domain, WAF, Cloud Armor) and signs unconditionally.
2. **Domain name registered + TLS certificate** — managed certificate via cert-manager or GCP-managed; DNS verified.
3. **Cloud Armor / WAF policy** — evaluated and applied with IP allowlists, rate-limiting rules, OWASP Top 10 baseline.
4. **Public LoadBalancer or GKE Ingress** — configured with `deletion_protection = true`; access logging enabled.
5. **Network Security Policy** — restrict ingress sources; deny default; allow only public LB + bastion.
6. **Sustained load test against public endpoint** — 30+ minutes, all alert types, real receivers, saturation point measured.
7. **Runbook for public ingress management** — documented and reviewed (teardown, incident response, certificate rotation).
8. **External pentest of public surface** — A-07 completed with no open CRITICAL/HIGH findings.

**No-enable condition:** Do NOT enable public ingress if any of the following are true:
- A-03 is not unconditionally signed by a named external SRE.
- A-04 is not unconditionally signed by a named external security reviewer.
- A-07 is not closed (external pen test executed, remediated, retested, signed).
- Domain/TLS/Cloud Armor are not fully provisioned and verified.
- No business need is documented and approved.

**Enabling public ingress automatically triggers reopening of A-03, A-04, A-07 per ADR-16 §4.**

---

## 4. Safe Execution Lanes (Can Start Now)

These lanes can be executed without external reviewers or public infrastructure. They improve the private-only system and build evidence for future public gates.

---

### Lane 1 — JWT Rotation (Dual-Key Support)

**Evidence:** JWT dual-key support implemented at commit `968855c` (`JWT_SECRET` + `JWT_SECRET_PREVIOUS`). Code support exists; rotation not yet executed.

**Preconditions:**
- GSM `JWT_SECRET` exists and is the current active signing key.
- `JWT_SECRET_PREVIOUS` env var support is deployed (confirmed in `auth.rs`).
- No maintenance window needed for JWT rotation (graceful: old tokens continue validating during transition).

**High-level steps:**
1. Generate new JWT signing secret (e.g., `openssl rand -base64 64`).
2. Set GSM `jwt-secret-previous` to current `jwt-secret` value.
3. Set GSM `jwt-secret` to new generated secret.
4. Force ESO sync (`kubectl annotate externalsecret app-secrets force-sync=...`).
5. Verify hash match (old vs new without printing secrets).
6. Rolling restart Deployment (`kubectl rollout restart deployment/intent-api`).
7. Smoke test with **new** token (should pass).
8. Smoke test with **old** token (should still pass — `JWT_SECRET_PREVIOUS` validates it).
9. Wait for token TTL (default 24h) or proactively invalidate old sessions.
10. Remove `jwt-secret-previous` from GSM after grace window.
11. Update `docs/09-operations/08-secrets-inventory.md` with rotation date and key version.

**Success markers:**
- `JWT_SECRET` hash matches GSM new version.
- `JWT_SECRET_PREVIOUS` hash matches GSM old version.
- New pod starts without JWT errors.
- Health/ready endpoints pass with both old and new tokens.
- No 401 spikes in application logs.

**Rollback:**
- Revert GSM `jwt-secret` to previous value.
- Force ESO sync + rolling restart.
- Old tokens remain valid throughout.

**Risk:** LOW. Graceful dual-key support means no downtime; old tokens remain valid during transition.

---

### Lane 2 — NATS JetStream (GCP Provisioning + Stage 1 Pilot)

**Evidence:**
- Local consumer code delivered (`CheckpointCreatorConsumer`, `SnapshotCreatorConsumer`, `NotifierConsumer`, `DlqMetricsWorker`, `DlqReplayWorker`, `ConsumerRegistry`).
- ADR-15 (`docs/13-adrs/15-nats-per-tenant-streams.md`) staged migration design complete.
- Env gates: `INTENT_API_NATS_CONSUMER`, `INTENT_API_NATS_FULL_CONSUMER`, `INTENT_API_NATS_DLQ_WORKER`, `INTENT_API_NATS_DLQ_REPLAY_WORKER`.

**Gaps:**
- No NATS server provisioned on GCP.
- No JetStream streams or consumers configured in production.
- No per-tenant stream topology.
- No TLS/mTLS for NATS.
- No ACLs or tenant-scoped subjects.

**Stage 1 — Single Tenant / Single Consumer Pilot:**
1. Provision NATS server on GKE (StatefulSet with persistent volume) OR evaluate Google Cloud Pub/Sub as managed alternative.
2. Configure JetStream with single stream (`INTENT_STREAM`) and single consumer (`CheckpointCreatorConsumer`).
3. Use a single tenant (synthetic tenant 1) for pilot.
4. Configure TLS for NATS (cert-manager self-signed or Google-managed cert).
5. Update `app-secrets` with `NATS_URL` (TLS-enabled internal endpoint).
6. Enable `INTENT_API_NATS_CONSUMER=true` in Deployment env.
7. Verify: consumer connects, checkpoints created, no DLQ overflow.
8. Run for 24h; observe metrics and logs.
9. Add `DlqMetricsWorker` behind `INTENT_API_NATS_DLQ_WORKER=true`; verify DLQ depth/age gauges appear in Prometheus.

**Stage 2–4** (per ADR-15): deferred until Stage 1 is stable and A-03 SRE signoff is obtained.

**Success markers:**
- NATS pod Ready, JetStream enabled.
- Consumer pod logs show successful checkpoint creation.
- Prometheus metrics `intent_api_dlq_messages_current` and `intent_api_dlq_message_age_seconds` emitted (if DLQ worker enabled).
- No connection errors or message loss in 24h pilot.
- Graceful shutdown on SIGTERM (consumer stops polling, no hung goroutines).

**Risk:** LOW for Stage 1 (single tenant, single consumer, behind env gate). No public surface change.

---

### Lane 3 — S3 / GCS Forensic Storage Validation

**Evidence:**
- Chain-hash algorithm (ADR-14, `chain_hash.rs`) delivered with tests.
- `S3BundleStorage` seam exists (`crates/forensic-service/src/s3_bundle_storage.rs` or equivalent).
- GCS bucket `ire-prod-ferrum-497801-production-template-ed2c5bdd` exists with retention policy.
- `FORENSIC_BUNDLE_STORAGE=s3` env gate exists.

**Caveat:** GCS retention policy is NOT S3 Object Lock compliance mode. For strict legal-hold / tamper-evidence requirements, actual AWS S3 Object Lock or a GCP equivalent may be needed. For private-only solo operation, GCS retention + chain-hash is a sufficient starting point.

**Validation plan:**
1. Create a dedicated GCS bucket for forensic bundles with uniform bucket-level access and retention policy (e.g., 30 days minimum).
2. Enable `FORENSIC_BUNDLE_STORAGE=s3` in staging environment.
3. Create a forensic bundle via `POST /forensic/bundle`.
4. Verify bundle is stored in GCS with `BundleIntegrity` metadata (including `previous_bundle_hash` chain-hash).
5. Download bundle via `GET /forensic/bundles/:id/download` and verify integrity hash matches.
6. Attempt to delete the object (should fail due to retention policy).
7. Verify chain-hash: download two sequential bundles, assert `bundle_N.previous_bundle_hash == hash(bundle_N-1)`.
8. Document the validation in `infrastructure/production/README.md` under Forensic Storage Validation.

**Success markers:**
- Bundle creation returns 201 with `storage_url` pointing to GCS.
- Download returns the exact bytes with matching integrity hash.
- GCS retention policy prevents deletion during retention period.
- Chain-hash links bundles in correct order.
- No secrets or credentials in logs or error responses.

**Risk:** LOW (staging validation only; no production data).

---

### Lane 4 — Staging Load / SLO Formalization

**Evidence:**
- 30-minute staging business-path load passed (8934 requests, 0% failure, p95 17.71ms, avg 6.97ms).
- Receiver validation during load passed (manual Alertmanager API alert + synthetic Prometheus rule firing).
- k6 Job manifests exist in `infrastructure/production/k6/`.

**Gaps:**
- No formal SLO document with committed targets.
- No committed k6 scripts in repo (jobs were ad-hoc).
- No real SLO/SLA rule breach validated (only synthetic rule).
- No error budget burn or latency threshold alert tested.
- No saturation point measured (5 VUs is modest).

**Execution plan:**
1. **Formal SLO doc:** Create `docs/11-quality/slo-targets.md` with:
   - Availability target (e.g., 99.9% over 30 days)
   - Latency targets (p50, p95, p99 for key endpoints)
   - Error budget (e.g., 0.1% error rate over 30 days)
   - Compensating timeout / retry policies
   - **Caveat:** These are staging/private-only targets; production SLOs require A-03 SRE signoff.
2. **Committed k6 scripts:** Add `k6-staging-business-path.js` to `infrastructure/production/k6/` with:
   - Setup: create two intents, two versions
   - Loop: get intent, list versions, get version, diff, rebase-preview, side-effects, graph nodes, health
   - Thresholds: p95 < 100ms, error rate < 0.1%, http_req_failed == 0%
3. **Escalate load:**
   - 5 VUs → 20 VUs → 50 VUs (1-hour sustained each)
   - Measure saturation point (CPU/memory limits, DB connection pool exhaustion, HPA scaling)
4. **Real SLO alert rules:** Add Prometheus recording rules and alert rules:
   - `intent_api_p95_latency > 100ms` for 5m → `warning`
   - `intent_api_error_rate > 0.1%` for 5m → `warning`
   - `intent_api_availability < 99.9%` for 30m → `critical`
   - Validate that Alertmanager fires real (not synthetic) notifications during load.
5. **Document results:** Update `infrastructure/production/README.md` with load test results, SLO targets, and alert rule validation.

**Success markers:**
- SLO doc committed and reviewed.
- k6 script committed and reproducible (`k6 run k6-staging-business-path.js`).
- 20 VU and 50 VU loads pass without errors.
- HPA scales correctly under load (CPU/memory thresholds hit, new pods created).
- Real Prometheus alert rules fire during load and deliver to Slack/email.
- Saturation point documented (max VUs before degradation).

**Risk:** LOW (staging only; no production traffic).

---

### Lane 5 — DR Maturity (Formal RPO/RTO Drill)

**Evidence:**
- PITR clone validated (clone ready ~20m, app ready ~6s).
- DR smoke passed (clone `dr-final-smoke-20260620051418`: health/ready ok, authenticated API 201/200).
- `docs/09-operations/07-backup-restore.md` documents RPO ≤ 5min, RTO ≤ 30min targets.

**Gaps:**
- RPO not empirically measured (Cloud SQL WAL streaming lag not observed).
- RTO not formally measured as a timed drill with documented procedure.
- No live-traffic cutover simulation (always non-destructive clone).
- No DR runbook appendix with measured wall-clock times.

**Execution plan:**
1. **Formal timed RTO drill:**
   - Step 1: Document the exact procedure (clone creation, app deployment switch, health verification, auth verification).
   - Step 2: Time each step with `date +%s` markers.
   - Step 3: Run the drill end-to-end; record `DR_CLONE_READY_SECONDS`, `DR_APP_READY_SECONDS`, `DR_AUTH_VERIFY_SECONDS`, `DR_TOTAL_SECONDS`.
   - Step 4: Assert `DR_TOTAL_SECONDS < 1800` (30 min RTO target).
   - Step 5: Publish results as DR runbook appendix.
2. **RPO measurement:**
   - Query Cloud SQL `pg_stat_archiver` or Cloud SQL logs for WAL archiving lag.
   - Record maximum observed lag during normal operation and under load.
   - Assert max lag < 300 seconds (5 min RPO target).
   - Document in DR runbook.
3. **Live-traffic cutover simulation (optional, higher risk):**
   - Redirect internal LB traffic to a clone-deployed app instance temporarily.
   - Measure downtime window (should be < 1 minute if RollingUpdate is used).
   - **Caution:** This affects live traffic; only do this if the system is truly private and downtime is acceptable.

**Success markers:**
- Formal RTO drill completed with documented wall-clock times.
- RTO ≤ 30 minutes demonstrated empirically.
- RPO ≤ 5 minutes demonstrated empirically (or documented from Cloud SQL SLA).
- DR runbook appendix published with step-by-step procedure and measured times.
- No data loss during any drill.

**Risk:** LOW for clone-only drill (non-destructive). MODERATE for live-traffic cutover (downtime risk).

---

### Lane 6 — CI/CD Audit Trail Decision

**Current state:** GitHub Actions CI intentionally disabled. Local gates (`verify-fast.sh`, `just verify-fast`) are source of truth. `smoke.yml` runs on PRs for lightweight gating. Docker images built and pushed manually.

**Decision required:** Keep local-only vs. enable remote CI / SBOM / signing.

**Options:**

| Option | Description | When to Choose | Cost / Effort |
|--------|-------------|----------------|---------------|
| **A. Keep local-only** | Continue with `verify-fast.sh` + manual image builds. | Solo private operation continues; no collaborators expected. | Zero. |
| **B. Enable remote CI for audit trail** | Enable GitHub Actions `ci.yml` on `workflow_dispatch` + `push` to main. | Before onboarding a collaborator; before any third-party requests build evidence. | Low (existing workflows; just enable triggers). |
| **C. Add SBOM + artifact signing** | Generate SBOMs (Syft/Grype), sign images (Sigstore/cosign), store attestations. | Before commercial/enterprise readiness; before customer asks for supply-chain evidence. | Medium (tooling setup, key management). |
| **D. Full CI/CD pipeline** | Remote CI, automated deploy to staging on merge, manual approval for prod, GitOps (Argo/Flux). | Before public production with multiple contributors. | High (significant infrastructure + process). |

**Recommended decision for current phase:** Option A (keep local-only). Revisit Option B before onboarding any collaborator. Revisit Option C before any commercial/enterprise discussion. Revisit Option D only after public ingress is enabled and A-03/A-04/A-07 are closed.

**No action required now.** Document the decision in `docs/09-operations/02-ci-cd.md` if Option B/C/D is chosen later.

---

## 5. Recommended Execution Order

### Phase A (Immediate — No External Dependencies)

These lanes can start today without any external reviewer or vendor engagement.

1. **Lane 1: JWT Rotation** — Execute dual-key rotation (low risk, no downtime, validates rotation infrastructure).
2. **Lane 5: Formal RTO Drill** — Non-destructive clone+app timed drill (builds DR evidence, closes FIND-004 gap partially).
3. **Lane 4: Committed k6 Scripts + SLO Doc** — Document targets, commit scripts, run 20 VU escalation (builds load test evidence, closes FIND-001 gap partially).
4. **Lane 3: GCS Forensic Validation** — Validate bundle create/verify/download/chain-hash in staging (builds forensic evidence).

### Phase B (Short-Term — Infrastructure Provisioning)

These require GCP infrastructure changes but no external reviewers.

5. **Lane 2: NATS Stage 1 Pilot** — Provision NATS on GKE, single tenant, single consumer, 24h validation.
6. **Lane 4: Real SLO Alert Rules + 50 VU Load** — Add Prometheus rules, validate firing under load, measure saturation.
7. **Lane 5: RPO Measurement** — Query Cloud SQL WAL lag, document empirical RPO.

### Phase C (Medium-Term — External Engagement)

These require external vendors, reviewers, or significant business decisions.

8. **A-07 Vendor Engagement** — Select vendor, sign NDA/SOW, execute pen test, remediate, retest, close A-07.
9. **A-03/A-04 External Re-Signoff** — After Phase A/B evidence is collected, engage external SRE and security reviewers for unconditional signoff.
10. **Public Ingress Decision** — Only after A-03, A-04, A-07 are unconditionally closed AND business need is documented. See §3 prerequisites.
11. **CI/CD Upgrade (Option B/C/D)** — Only after public ingress or collaborator onboarding is planned.

### Phase D (Long-Term — Enterprise / Commercial Readiness)

These are not required for private solo operation but are needed for commercial use.

12. **SOC2 / GDPR / ISO27001 Audit** — Engage compliance auditor; implement controls; obtain report.
13. **Team / On-Call Structure** — Define roles, escalation matrix, on-call rotation.
14. **SLA / SLO Commitments** — Contractual availability/latency targets with customer-facing penalty clauses.
15. **SBOM / Dependency Audit** — Full software bill of materials; vulnerability scanning; license compliance.
16. **Incident Response Drills** — Tabletop exercises with team; validate runbooks under simulated outage.
17. **Data Deletion / Residency** — Implement GDPR right-to-erasure; verify data residency controls.
18. **Customer Documentation** — Public-facing docs, API guides, support SLAs.

---

## 6. Forbidden Claims

| Forbidden Claim | Allowed Replacement |
|----------------|-------------------|
| `Production-ready` | `Private-only solo operation approved with conditions; public production gates open` |
| `A-07 passed` | `A-07 WAIVED-SOLO / PRIVATE-ONLY; external pen test not executed` |
| `A-03 approved` | `A-03 APPROVED WITH CONDITIONS — PRIVATE-ONLY; external re-signoff not obtained` |
| `A-04 approved` | `A-04 APPROVED WITH CONDITIONS — PRIVATE-ONLY; external re-signoff not obtained` |
| `External security review passed` | `ZAP self-scan prep only (0 FAIL, 1 WARN); external review not obtained` |
| `SRE signoff obtained` | `External SRE signoff pending; historical conditional signoff on record` |
| `Load testing passed` | `Staging 30-min business-path load passed; production/public-ingress load not done` |
| `DR validated` | `Clone+app DR smoke passed; formal RPO/RTO measurement pending` |
| `Forensic storage production-ready` | `GCS bucket with retention exists; chain-hash delivered; Object Lock not deployed` |
| `NATS production-ready` | `Local consumer gates delivered; no GCP NATS deployed` |
| `JWT rotation complete` | `API key rotation validated; JWT dual-key ready but not yet rotated` |
| `CI/CD complete` | `Local gates accepted; remote CI disabled by design` |
| `Commercial-ready` | `Private-only; commercial readiness requires SOC2/GDPR/team/SLA/SBOM/IR drills` |
| `Enterprise-ready` | `Private-only; enterprise readiness requires external audits, team, SLA, customer docs` |

---

## 7. Related Documents

| Document | Relationship |
|----------|--------------|
| `docs/09-operations/12-authorization-signoff-packet.md` | Source sign-off statuses and evidence citations for all 10 gates |
| `docs/13-adrs/16-solo-private-operation-waiver.md` | ADR-16 waiver terms, reopening triggers, review expiry (2026-09-17) |
| `docs/08-security/08-external-pentest-engagement.md` | A-07 engagement packet for vendor handoff |
| `docs/09-operations/11-public-ingress-decision.md` | Public ingress decision (private-only) and prerequisites checklist |
| `docs/09-operations/10-external-review-packet.md` | Historical DuongNguyen 2026-06-15 review; A-03/A-04 tracker |
| `docs/13-adrs/15-nats-per-tenant-streams.md` | ADR-15 NATS staged migration design |
| `docs/13-adrs/14-forensic-chain-hash.md` | ADR-14 chain-hash algorithm design |
| `infrastructure/production/README.md` | Canonical applied-scaffold evidence record (GCP, GKE, Cloud SQL, Prometheus, ESO, load tests, DR) |
| `docs/09-operations/05-runbooks.md` | Operational runbooks RB1–RB14 (rotation, DR, incident response) |
| `docs/09-operations/07-backup-restore.md` | PITR, pg_dump/pg_restore, DR procedures |
| `docs/09-operations/08-secrets-inventory.md` | Secret inventory and rotation procedures |
| `docs/10-delivery/25-remaining-todo-list.md` | Consolidated remaining items view (points to this plan) |
| `docs/10-delivery/20-project-completion-roadmap.md` | Project completion roadmap with P3 reframed |

---

## 8. Update Log

| Date | Updated By | Changes |
|------|------------|---------|
| 2026-06-21 | BrianNguyen (via authorized assistant fixer) | Initial post-signoff execution plan created. Status snapshot derived from `12-authorization-signoff-packet.md`. External artifact intake checklist for A-07/A-03/A-04. Gated public path prerequisites. Six safe execution lanes (JWT rotation, NATS pilot, S3/GCS forensic, staging load/SLO, DR drill, CI/CD decision). Recommended execution order (Phase A–D). Forbidden claims table. No production-ready claim. No external signoff claim. |
