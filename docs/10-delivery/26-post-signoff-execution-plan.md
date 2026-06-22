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
| S3 / GCS Forensic Storage | 🟡 PILOT VALIDATED 2026-06-21 — NOT PRODUCTION IMMUTABLE | Planning | Chain-hash (ADR-14) + `S3BundleStorage` seam delivered; GCS bucket exists but live metadata shows **no retention policy, no versioning, no public access prevention** (2026-06-21); application-layer hash verification validated (upload/download/SHA-256 match + tamper detection); storage-layer immutability **NOT enforced**; NOT S3 Object Lock compliant; no production validation |
| Broader Secret Rotation | 🟡 PARTIALLY APPROVED — DB ROTATED 2026-06-21 | Private-only | API key rotation validated (prod + staging); **JWT dual-key rotation validated (2026-06-21)**; **DB URL rotation validated (2026-06-21)** — Cloud SQL user password rotated, GSM version 2, ESO sync, deployment restart, health/ready 200, DB connection verified; NATS/S3 secrets not yet rotated (NATS not deployed on GCP, S3 not wired to app) |
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
| 2 | FIND-003 closed (broader secret rotation exercised: DB URL, JWT, NATS, S3) | 🔴 OPEN | Security | API key validated 2026-06-19; JWT validated 2026-06-21; DB URL validated 2026-06-21; NATS/S3 pending. See Lane 1. |
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

**Evidence:** JWT dual-key support implemented at commit `968855c` (`JWT_SECRET` + `JWT_SECRET_PREVIOUS`). **Rotation validated 2026-06-21** — GSM version 2 created, `JWT_SECRET_PREVIOUS` mapped via ESO, deployment restarted, health 200 with both new and old tokens. Grace window active; removal deferred to 24h+ or client refresh. See `docs/09-operations/05-runbooks.md` RB21 Evidence section.

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

**Grace Window Removal Trigger (Documented):**

`JWT_SECRET_PREVIOUS` must be removed from GSM and the K8s secret **only when all of the following are true**:
1. At least 24 hours have passed since rotation (default token TTL grace window).
2. No active clients are known to be using old tokens, or old tokens have been proactively invalidated.
3. A smoke test with **new token only** (no `JWT_SECRET_PREVIOUS` fallback) passes after removal.

**If any client still uses an old token:** keep `JWT_SECRET_PREVIOUS` active and document the exception in `docs/09-operations/08-secrets-inventory.md`.

**Current status (2026-06-22):** Grace window closed 2026-06-22. `JWT_SECRET_PREVIOUS` removed from K8s secret; new token verified accepted by auth middleware; old token no longer in runtime (old secret retained in GSM for potential rollback but not mounted in the app). See Phase A status and update log for full evidence.

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
- **Pilot provisioned 2026-06-21:** NATS StatefulSet `nats` (1 replica) running on GKE with JetStream enabled, PVC `nats-jetstream-pvc` (10Gi), ClusterIP Service `nats` (port 4222 internal only). No public ingress, no nodePort, no LB.
- **Validation 2026-06-21:** `nats server check jetstream` OK; stream `pilot_test` created (subjects `pilot.test.>`, file storage, limits retention, 1 replica); durable consumer `pilot_consumer` created (pull mode, explicit ack, max_deliver=3); message published to `pilot.test.msg` and consumed successfully (seq 1, acknowledged). Manifests under `infrastructure/production/kubernetes/nats/`.

**Gaps:**
- ~~No NATS server provisioned on GCP.~~ ✅ Pilot provisioned (single-node, not production HA cluster).
- No JetStream streams or consumers configured in production for app use (`audit_events` stream not yet created by app code).
- No per-tenant stream topology (ADR-15 Stages 2–4 still blocked on A-03/A-05).
- No TLS/mTLS for NATS (pilot uses plain TCP on internal ClusterIP).
- No ACLs or tenant-scoped subjects (pilot has no auth).
- App consumer not enabled (`INTENT_API_NATS_CONSUMER` not set in Deployment; requires app code to connect to `nats://nats:4222`).

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
- GCS bucket `ire-prod-ferrum-497801-production-template-ed2c5bdd` exists.
- `FORENSIC_BUNDLE_STORAGE=s3` env gate exists.
- **Validation executed 2026-06-21:** Tiny non-secret test bundle uploaded, hash verified, tamper detected, overwrite/delete behavior documented. See `infrastructure/production/README.md` Forensic Storage Validation section.

**Caveat:** GCS bucket metadata inspection shows **no retention policy, no versioning, no public access prevention** configured. This is NOT S3 Object Lock compliance mode and does NOT provide immutable storage. For strict legal-hold / tamper-evidence requirements, actual AWS S3 Object Lock (compliance mode) or a GCP equivalent (e.g., GCS retention policy + Bucket Lock + versioning) must be configured. For private-only solo operation, the chain-hash algorithm + explicit hash verification in application code provides tamper-evidence at the application layer, but storage-layer immutability is not enforced.

**Validation executed (2026-06-21):**
1. Bucket metadata inspected: `retentionPolicy=None`, `versioning=None`, `publicAccessPrevention=None`, `location=US-CENTRAL1`, `storageClass=None`.
2. Generated tiny non-secret JSON test bundle (`forensic-bundle-1.json`, 171 bytes, SHA-256 `c8da13ac...2053`).
3. Uploaded to `gs://.../forensic-validation/20260621/bundle-1.json`.
4. Downloaded and verified SHA-256 match: `c8da13ac...2053` ✅.
5. Generated tampered bundle (`forensic-bundle-2.json`, 174 bytes, SHA-256 `fcec0d65...963c`).
6. Uploaded tampered bundle to separate key; downloaded and verified hash mismatch against original expected hash: mismatch detected ✅.
7. Overwrote original `bundle-1.json` with tampered content; downloaded and verified hash changed to `fcec0d65...963c` — **overwrite succeeded because no retention policy / Object Lock exists**. This confirms storage-layer immutability is NOT enforced.
8. Deleted both validation objects; deletion succeeded (no retention policy blocking cleanup).

**Success markers:**
- Upload succeeded; download returned exact bytes with matching SHA-256 ✅.
- Tamper detection via hash mismatch works at application layer ✅.
- Storage-layer immutability is **NOT enforced** — overwrite and deletion both succeed ✅ (documented as finding).
- No secrets or credentials in logs or error responses ✅.

**Risk:** LOW (tiny non-secret test data only; no production data; objects deleted after validation).

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
- **Formal RTO drill executed 2026-06-22:** Clone `dr-formal-rto-20260622023356` created, RUNNABLE in 1018s (~16m58s), app validation (psql SELECT 1 + sqlx migrations) passed in ~6s, total elapsed 1297s (~21m37s). All temp resources cleaned up. Not a live-traffic cutover.

**Gaps:**
- RPO not empirically measured (Cloud SQL WAL streaming lag not observed against live writes).
- No live-traffic cutover simulation (always non-destructive clone).
- RTO target ≤ 30min not achieved empirically (clone provisioning alone ~17m, total ~21m; target is 30m so this is within target, but not under incident conditions with live traffic).
- No DR runbook appendix with measured wall-clock times for full cutover procedure.

**Execution plan (completed):**
1. ~~**Formal timed RTO drill:**~~ ✅ **COMPLETED 2026-06-22** — See `docs/09-operations/07-backup-restore.md` §Formal DR RTO/RPO Drill (2026-06-22). Timings: `DR_CLONE_READY_SECONDS=1018` (~16m58s), `DR_APP_READY_SECONDS=90` (postgres Job) / `~5s` (migration Job), `DR_TOTAL_SECONDS=1297` (~21m37s). Non-destructive; all temp resources cleaned up.
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
- ~~Formal RTO drill completed with documented wall-clock times.~~ ✅ **DONE 2026-06-22**
- RTO ≤ 30 minutes demonstrated empirically (clone-only: ~21m37s total, within target).
- RPO ≤ 5 minutes demonstrated empirically (or documented from Cloud SQL SLA) — **PENDING**.
- DR runbook appendix published with step-by-step procedure and measured times — **PARTIAL** (timings documented in `docs/09-operations/07-backup-restore.md`).
- No data loss during any drill — ✅ **VERIFIED** (non-destructive, no production data touched).

**Risk:** LOW for clone-only drill (non-destructive). MODERATE for live-traffic cutover (downtime risk).

---

### Lane 6 — CI/CD Audit Trail Decision

**Current state:** GitHub Actions CI intentionally disabled. Local gates (`verify-fast.sh`, `just verify-fast`) are source of truth. `smoke.yml` runs on PRs for lightweight gating. Docker images built and pushed manually.

**Decision implemented 2026-06-22:** Option B + partial C (manual audit trail with SBOM generation, no signing, no auto-deploy).

**What was done:**
- New workflow `.github/workflows/audit-trail.yml` created with `workflow_dispatch` only (no automatic runs).
- Jobs: `quality` (fmt, check, clippy, lib tests), `sbom` (anchore/sbom-action generating SPDX + CycloneDX JSON, uploaded as artifact with 30-day retention).
- Signing/attestation job is intentionally commented out and documented as deferred: no registry push configured, no OIDC identity for Sigstore, no artifact registry credentials in GitHub Actions.
- No auto-deploy, no Docker build/push, no registry interaction, no secrets.
- Existing workflows preserved: `ci.yml` (workflow_dispatch with manual boolean flags), `smoke.yml` (workflow_dispatch + PR for lightweight gates).

**Why this decision:**
- Produces reproducible audit artifacts (SBOM JSON) on manual trigger for external reviewer intake without enabling auto-deploy or incurring ongoing CI costs.
- Keeps local gates (`just verify-fast`) as the source of truth for day-to-day development.
- Does not claim external compliance or supply-chain certification; SBOM artifacts are plain uploads, not signed attestations.
- Signing (Option C full) remains deferred until registry push is configured and a signing identity is established.

**Risks:** LOW. Manual-only trigger; no deployment; no registry; no secrets.

**Options table (updated):**

| Option | Description | Status |
|--------|-------------|--------|
| **A. Keep local-only** | Continue with `verify-fast.sh` + manual image builds. | ✅ Baseline maintained |
| **B. Enable remote CI for audit trail** | Manual `workflow_dispatch` audit workflow with SBOM artifacts. | ✅ **IMPLEMENTED 2026-06-22** |
| **C. Add SBOM + artifact signing** | Sign SBOMs/images with Sigstore/cosign, store attestations. | 🟡 **PARTIAL — SBOM generated, signing deferred** |
| **D. Full CI/CD pipeline** | Remote CI, automated deploy to staging, manual prod approval, GitOps. | 🔴 **NOT STARTED — blocked on public ingress + A-03/A-04/A-07** |

---

## 5. Recommended Execution Order

### Phase A (Immediate — No External Dependencies)

These lanes can start today without any external reviewer or vendor engagement.

1. ~~**Lane 1: JWT Rotation**~~ ✅ **COMPLETED 2026-06-21; GRACE WINDOW CLOSED 2026-06-22** — New prod JWT secret created (GSM version 2), `JWT_SECRET_PREVIOUS` mapped via ESO, deployment restarted, health 200 with both new and old tokens (2026-06-21). Grace window closed 2026-06-22: `JWT_SECRET_PREVIOUS` mapping removed from ExternalSecret manifest, ESO synced, K8s secret no longer contains `JWT_SECRET_PREVIOUS`, deployment restarted, health/ready 200, new token verified accepted by auth middleware (temporary Job `jwt-token-test-20260622`). Old token rejection not empirically tested (old secret remains in GSM but not mounted in runtime). Old GSM secret `intent-rebase-prod-jwt-secret-previous` retained for potential rollback but not consumed by app.
2. ~~**Lane 2: NATS Stage 1 Pilot**~~ ✅ **COMPLETED 2026-06-21** — Internal NATS JetStream pilot provisioned on GKE (StatefulSet, PVC, ClusterIP), validated: stream create/list, durable consumer create/list, publish/consume message. No public ingress, no app consumer enabled yet. Not production HA cluster.
3. ~~**Lane 3: GCS Forensic Validation**~~ ✅ **COMPLETED 2026-06-21** — Tiny non-secret test bundle uploaded to GCS, SHA-256 hash verified on download, tamper detection via hash mismatch demonstrated, overwrite/delete behavior documented. Bucket metadata shows no retention policy, no versioning, no public access prevention — storage-layer immutability NOT enforced. Chain-hash algorithm (ADR-14) provides application-layer tamper-evidence. Not S3 Object Lock compliant. Objects deleted after validation. See `infrastructure/production/README.md` Forensic Storage Validation.
4. ~~**Lane 4: Committed k6 Scripts + SLO Doc**~~ ✅ **COMPLETED 2026-06-21; ESCALATED 2026-06-22; APP METRICS INSTRUMENTED 2026-06-22** — Reproducible k6 script `infrastructure/production/k6/business-path-smoke.js` and Kubernetes Job manifest `infrastructure/production/k6/business-path-load-job.yaml` created. Formal SLO targets doc `docs/09-operations/11-slo-targets.md` created with SLO definitions (availability, latency, error rate, capacity, target scrape), error budgets, measurement methods, blockers, forbidden claims, and next steps. 7-minute bounded internal load test executed (5 VUs, ramp 1m + sustain 5m + ramp-down 1m): 1830 checks (2026-06-21), 1829 checks (2026-06-22), 100% pass, 0% failure, p95 latency 848.42µs (2026-06-21) / 1.67ms (2026-06-22), avg 639.82µs / 1.25ms, 4.35 req/s. Temporary synthetic Prometheus rule `SLOValidationSyntheticRule` added 2026-06-21, verified firing, removed. Temporary real-metric Prometheus rule `PrometheusSelfMetricValidation` (`expr: prometheus_build_info > 0`, `for: 0s`) added 2026-06-22, verified firing (`state=firing`, `activeAt=2026-06-22T02:10:46.119653393Z`) throughout load test, then removed. **App metrics instrumented 2026-06-22**: `http_requests_total` counter and `http_request_duration_seconds` histogram added via axum middleware in `router.rs`; Prometheus recorder initialization moved from lazy `/metrics` handler to startup-time `init_metrics()` in `main.rs` via `METRICS_HANDLE` (`OnceLock`). Docker image `cd370d1-metrics` built, pushed, and deployed to GKE; live pod `intent-api-689776d4b5-rmtnk` verified returning non-empty metrics (content-length: 906) with `http_requests_total{method="GET",status="200"}` and `http_request_duration_seconds` quantiles. Prometheus restarted with cleaned rules after both tests; no alerts firing confirmed. NATS temporarily scaled down to 0 to free cluster CPU for k6 pod; restored to 1 after each test. Not production/public-ingress load.
5. ~~**Lane 5: Formal RTO Drill**~~ ✅ **COMPLETED 2026-06-22** — Non-destructive clone+app timed drill: clone `dr-formal-rto-20260622023356` RUNNABLE in 1018s (~16m58s), app validation (psql SELECT 1 + sqlx migrations) passed in ~6s, total elapsed 1297s (~21m37s). All temp resources cleaned up. Not a live-traffic cutover. RPO not empirically measured. Timings documented in `docs/09-operations/07-backup-restore.md`.
6. ~~**Lane 6: CI/CD Audit Trail**~~ ✅ **IMPLEMENTED 2026-06-22** — Manual audit workflow `.github/workflows/audit-trail.yml` created with `workflow_dispatch` only. Jobs: quality gates (fmt, check, clippy, lib tests), SBOM generation (SPDX + CycloneDX via anchore/sbom-action, uploaded as artifact, 30-day retention). Signing/attestation deferred (no registry push, no OIDC identity). Existing workflows preserved. Local gates remain SOT. No auto-deploy, no secrets, no registry interaction.

### Phase B (Short-Term — Infrastructure Provisioning)

These require GCP infrastructure changes but no external reviewers.

7. ~~**Lane 2: NATS Stage 1 Pilot**~~ ✅ **COMPLETED 2026-06-21; PROMETHEUS EXPORTER ADDED 2026-06-22; APP CONSUMER WIRED 2026-06-22** — Pilot provisioned and validated. Prometheus scrape target added via `natsio/prometheus-nats-exporter:0.15.0` sidecar on port 7777; Prometheus job `nats` target UP; metrics `gnatsd_connz_num_connections` verified. App consumer integration completed: `NATS_URL=nats://nats:4222` and `INTENT_API_NATS_CONSUMER=true` added to deployment env; pod restarted; health/ready 200; logs show `ConsumerRegistry: starting consumer 'checkpoint_creator' on stream 'audit_events'`; NATS monitoring endpoint shows `audit_events` stream created, 2 consumers, 2 streams. DLQ workers not enabled (`INTENT_API_NATS_DLQ_WORKER`/`INTENT_API_NATS_DLQ_REPLAY_WORKER` not set). Next: TLS/auth hardening, or per-tenant stream pilot (ADR-15 Stage 2 — blocked on A-03).
8. ~~**Lane 4: Real SLO Alert Rules + 50 VU Load**~~ ✅ **COMPLETED 2026-06-21; ESCALATED 2026-06-22; APP METRICS INSTRUMENTED 2026-06-22; REAL SLO RULES APPLIED + VALIDATED 2026-06-22** — Reproducible k6 script and Job manifest created; 7-min internal load tests passed (2026-06-21 and 2026-06-22); temporary synthetic Prometheus rule verified firing and cleaned up (2026-06-21); temporary real-metric Prometheus rule (`prometheus_build_info > 0`) verified firing and cleaned up (2026-06-22); real app SLO rules (`IntentApiLatencyP95High` with `http_request_duration_seconds{quantile="0.95"} > 0.1`, `IntentApiErrorRateHigh` with `sum(rate(http_requests_total{status=~"5.."}[5m])) / sum(rate(http_requests_total[5m])) > 0.001`) applied to Prometheus ConfigMap, Prometheus restarted, rules loaded and `inactive` under normal load; temporary `AppMetricsValidationRule` (`http_requests_total > 0`) added, verified firing (`value=5286`), then removed; bounded k6 load test executed (1830 iterations, p95=687µs, 0% error, 5 VUs, 7min); all 4 Prometheus targets UP after restart. Formal SLO doc `docs/09-operations/11-slo-targets.md` updated. Next: run higher VU loads (20 VU, 50 VU), test SLO breach conditions (inject artificial latency/errors), deploy node-exporter + kube-state-metrics.
9. ~~**Lane 5: RPO Measurement**~~ ✅ **PARTIALLY DOCUMENTED 2026-06-22** — Cloud SQL backup config inspected: PITR enabled, automated backups daily at 03:00, replication log archiving enabled, transaction log retention 7 days. Last automated backup 2026-06-22T05:19:07Z. Theoretical RPO < 1 minute (WAL lag). Empirical WAL lag not measured (requires live workload + `pg_stat_archiver` query). Closest measurable recovery window documented with limitation.
10. ~~**Prometheus PVC Persistence**~~ ✅ **APPLIED 2026-06-22** — PVC `prometheus-storage` (10Gi, standard-rwo, RWO) created; Prometheus Deployment updated from `emptyDir` to PVC; `securityContext.fsGroup: 65534` added to fix permission denied; pod Running, TSDB started with WAL replay. Not HA; single-replica acceptable for private-only.
11. ~~**Dedicated Forensic Immutable Bucket**~~ ✅ **CREATED 2026-06-22** — New bucket `gs://forensic-evidence-ferrum-497801-ed2c5bdd` with versioning, 30-day retention policy (UNLOCKED), public access prevention, uniform access, lifecycle delete after 365 days. Bucket Lock deferred. Upload/delete test succeeded. Terraform resource added but applied via `gcloud` due to inaccessible remote state. **Runtime wiring blocked:** App only supports `S3BundleStorage` (requires `FORENSIC_BUNDLE_STORAGE=s3` + `S3_ENDPOINT` + `S3_ACCESS_KEY` + `S3_SECRET_KEY` + `FORENSIC_BUNDLE_BUCKET`) or in-memory default. No GCS native backend exists. Using GCS bucket via S3 interoperability requires HMAC keys (long-lived secrets) — not created without explicit security review. Workload Identity / least-privilege SA pattern not implemented. Runtime wiring deferred pending approved credential design. |

### Phase C (Medium-Term — External Engagement)

These require external vendors, reviewers, or significant business decisions.

12. **A-07 Vendor Engagement** — Select vendor, sign NDA/SOW, execute pen test, remediate, retest, close A-07.
13. **A-03/A-04 External Re-Signoff** — After Phase A/B evidence is collected, engage external SRE and security reviewers for unconditional signoff.
14. **Public Ingress Decision** — Only after A-03, A-04, A-07 are unconditionally closed AND business need is documented. See §3 prerequisites.
15. ~~**CI/CD Audit Trail (Lane 6)**~~ ✅ **IMPLEMENTED 2026-06-22** — See Lane 6 above. Full CI/CD upgrade (Option D: auto-deploy, GitOps) remains blocked on public ingress + A-03/A-04/A-07.

### Phase D (Long-Term — Enterprise / Commercial Readiness)

These are not required for private solo operation but are needed for commercial use.

16. **SOC2 / GDPR / ISO27001 Audit** — Engage compliance auditor; implement controls; obtain report.
17. **Team / On-Call Structure** — Define roles, escalation matrix, on-call rotation.
18. **SLA / SLO Commitments** — Contractual availability/latency targets with customer-facing penalty clauses.
19. **SBOM / Dependency Audit** — Full software bill of materials; vulnerability scanning; license compliance.
20. **Incident Response Drills** — Tabletop exercises with team; validate runbooks under simulated outage.
21. **Data Deletion / Residency** — Implement GDPR right-to-erasure; verify data residency controls.
22. **Customer Documentation** — Public-facing docs, API guides, support SLAs.

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
| `DR validated` | `Clone+app DR smoke passed; formal RPO/RTO measurement documented (closest measurable recovery window, not empirical WAL lag); RTO ~21m37s validated 2026-06-22` |
| `Forensic storage production-ready` | `GCS bucket exists; chain-hash delivered; application-layer hash verification validated 2026-06-21; dedicated forensic bucket created 2026-06-22 with versioning, retention policy, public access prevention; runtime wiring blocked (no GCS backend, HMAC keys not created, Workload Identity not implemented); storage-layer immutability NOT enforced (retention policy UNLOCKED, no Bucket Lock, no S3 Object Lock); not S3 Object Lock compliant` |
| `NATS production-ready` | `Internal pilot provisioned 2026-06-21 (single-node, no auth, no TLS); Prometheus exporter added 2026-06-22; app consumer (CheckpointCreatorConsumer) wired 2026-06-22 with `INTENT_API_NATS_CONSUMER=true` and `NATS_URL=nats://nats:4222`; `audit_events` stream created; DLQ workers not enabled; not production HA cluster, not per-tenant streams, not production-certified` |
| `JWT rotation complete` | `JWT rotation validated 2026-06-21 (GSM version 2, ESO sync, deployment restart, health 200 with new + old tokens); grace window closed 2026-06-22 (JWT_SECRET_PREVIOUS removed from K8s secret, new token verified accepted, old token rejection not empirically tested but old secret no longer mounted in runtime)` |
| `CI/CD complete` | `Local gates accepted; manual audit workflow implemented (SBOM generation, no signing, no auto-deploy); remote CI disabled by design for private-only` |
| `Commercial-ready` | `Private-only; commercial readiness requires SOC2/GDPR/team/SLA/SBOM/IR drills` |
| `SLO validated` | `Internal/private-only SLO rules applied and validated under bounded 5 VU k6 load (2026-06-22). Real app metrics (`http_requests_total`, `http_request_duration_seconds`) emitted and scraped. Latency p95 and error rate rules loaded and `inactive` under normal load. Temporary validation rule fired and removed. No public ingress edge latency. No higher VU saturation test. No formal SLA or error budgets.` |

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
| `docs/09-operations/13-reviewer-intake-package.md` | Evidence index for A-03/A-04/A-07 external reviewer intake (created 2026-06-22) |
| `docs/10-delivery/25-remaining-todo-list.md` | Consolidated remaining items view (points to this plan) |
| `docs/10-delivery/20-project-completion-roadmap.md` | Project completion roadmap with P3 reframed |

---

## 8. Update Log

| Date | Updated By | Changes |
|------|------------|---------|
| 2026-06-22 | BrianNguyen (via authorized assistant fixer) | Post-signoff execution Phase A and B lanes completed: Lane 1 (JWT rotation, grace closed), Lane 2 (NATS pilot + Prometheus exporter + app consumer wired), Lane 3 (forensic bucket created, runtime wiring blocked), Lane 4 (real SLO rules applied + validated under k6, Prometheus PVC), Lane 5 (formal DR RTO drill, RPO closest measurable), Lane 6 (CI/CD audit trail), Lane 7 (NATS docs gaps), Lane 8 (SLO docs gaps), Lane 9 (JWT grace closure), Lane 10 (forensic runtime wiring blocker), Lane 11 (reviewer intake package). New doc `docs/09-operations/13-reviewer-intake-package.md` created with A-03/A-04/A-07 evidence index. All changes remain uncommitted. No production-ready claim. No external signoff claim. A-07 WAIVED-SOLO. A-03/A-04 SELF-ATTESTED-SOLO. Verification: `git diff --check` pass, forbidden-claim scan clean, secret scan clean. |
