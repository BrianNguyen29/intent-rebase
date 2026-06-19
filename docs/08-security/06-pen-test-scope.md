# 06 — Penetration Testing Scope

**Status:** Accepted Internal Planning Artifact — internal planning acceptance only; not externally reviewed
**Phase:** Phase 3  
**Owner:** Security Team

---

## Purpose

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

---

## ZAP Baseline Self-Scan Evidence (2026-06-18)

> **Status:** PREP COMPLETED — This is **self-scan prep only** and does **NOT close A-07**. A named external tester is still required.

| Field | Value |
|-------|-------|
| **Tool** | OWASP ZAP Docker `ghcr.io/zaproxy/zaproxy:stable` |
| **Target** | Staging `intent-api` via port-forward `0.0.0.0:18081` → `http://host.docker.internal:18081` |
| **Report directory** | `/tmp/opencode/zap-a07-staging-20260618b` (not committed to repo) |
| **Reports** | `zap-report.html`, `zap-report.json`, `zap-report.md` |
| **Result** | `FAIL-NEW: 0`, `WARN-NEW: 2`, `PASS: 65`, `ZAP_RC: 2` |

### Warnings (non-blocking)

| Alert | Count | Context | Notes |
|-------|-------|---------|-------|
| `Content-Type Header Missing` [10019] | 3x | `/`, `/robots.txt`, `/sitemap.xml` | Unauthenticated 401 responses; not a security vulnerability for public endpoints that return 401 |
| `Non-Storable Content` [10049] | 3x | `/`, `/robots.txt`, `/sitemap.xml` | Unauthenticated 401 responses; cache-control headers are intentionally set for API responses |

### ZAP Re-Run After 401 Header Hardening (image `9e26aaa`)

| Field | Value |
|-------|-------|
| **Tool** | OWASP ZAP Docker `ghcr.io/zaproxy/zaproxy:stable` |
| **Target** | Staging `intent-api` (image `9e26aaa`) via port-forward `0.0.0.0:18081` → `http://host.docker.internal:18081` |
| **Report directory** | `/tmp/opencode/zap-a07-staging-9e26aaa-20260618` (not committed to repo) |
| **Result** | `FAIL-NEW: 0`, `WARN-NEW: 1`, `PASS: 66`, `ZAP_RC: 2` |

### Warnings (non-blocking)

| Alert | Count | Context | Notes |
|-------|-------|---------|-------|
| `Non-Storable Content` [10049] | 2x | `/`, `/robots.txt` | Unauthenticated 401 responses; **expected and security-intended** due to `Cache-Control: no-store` header added in 401 response hardening. This is accepted risk, not a vulnerability. |

### Interpretation

- **Content-Type Header Missing [10019] now PASS** (was 3x WARN in initial scan) — 401 response header hardening (`Content-Type: application/json` + `Cache-Control: no-store`) resolved this warning.
- **Non-Storable Content [10049] remains WARN** (reduced from 3x to 2x) — this is expected behavior on 401 responses that set `Cache-Control: no-store`. ZAP flags this because the response is not cacheable, which is the correct security posture for authenticated endpoints returning 401.
- **Zero failures** (`FAIL-NEW: 0`) means no HIGH/CRITICAL or MEDIUM findings.
- This scan does **not** cover: authenticated API surface, tenant isolation, cross-tenant data leakage, approval bypass, audit trail tampering, or runtime adapter injection. These require manual/expert testing.
- A-07 remains **OPEN** until a named external tester completes the full scope and delivers evidence per the Execution Readiness Addendum checklist.

### Authenticated ZAP API Scan Attempt (Blocked — Tool Limitation)

| Field | Value |
|-------|-------|
| **Date** | 2026-06-19 |
| **Objective** | Attempt authenticated ZAP API scan against staging OpenAPI surface using JWT header |
| **Method** | Canonical OpenAPI ZAP API scan (`zap-api-scan.py`) via pod port-forward with `-config replacer.full_list\(0\).description=auth1`, `-config replacer.full_list\(0\).matchtype=REQ_HEADER`, `-config replacer.full_list\(0\).matchstr=Authorization`, `-config replacer.full_list\(0\).regex=false`, `-config replacer.full_list\(0\).replacement=Bearer <JWT>` |
| **Attempts** | 3 attempts: (a) canonical OpenAPI 3.0.3 with explicit `servers: http://127.0.0.1:18089`, (b) clean OpenAPI copy with `servers`, (c) generated minimal OpenAPI with seeded staging endpoints |
| **Result** | All attempts failed before scan: `Number of Imported URLs: 0`, `Failed to import any URLs`, `ZAP_RC=3`, no JSON report produced |
| **Report dirs** | `/tmp/opencode/zap-auth-staging-20260619`, `/tmp/opencode/zap-auth-staging-20260619b`, `/tmp/opencode/zap-auth-minimal-20260619` |

**Interpretation:**
- This is a **tooling/import limitation**, not a security pass. ZAP's API scan failed to import URLs from the provided OpenAPI spec under all three attempted configurations.
- Authenticated ZAP scan **not completed**. No authenticated API surface was exercised by ZAP.
- The only ZAP evidence remains the **unauthenticated baseline self-scan** (0 FAIL, 1 WARN accepted) documented above.
- Manual authenticated API self-tests were performed during synthetic data seeding (create intents/versions/graph nodes/webhook via JWT), but these are functional tests, not security scanning.
- A-07 remains **OPEN**; external expert testing is still required for authenticated surface, tenant isolation, approval bypass, etc.

---

## Execution Readiness Addendum (2026-06-18)

> **Status:** PLAN DOCUMENTED — execution NOT started. A-07 remains 🔴 NOT APPROVED / OPEN until real external engagement completes.
>
> **A-07 Strategy Options (2026-06-19):**
> - **Preferred path:** Engage external pentester/vendor (HackerOne, Bugcrowd, or vetted freelance). Define scope (authenticated API surface, tenant isolation, cross-tenant leakage, approval bypass, audit tampering, runtime adapter injection), provide staging environment credentials, set execution window, require PDF/JSON report with CVSS scores, establish remediation/retest path. Budget and procurement required. **See `docs/08-security/08-external-pentest-engagement.md` for the full engagement packet.**
> - **Alternative path:** Formal waiver/re-scope for internal/private-only operation. This requires explicit documented acceptance that no production-ready claim can be made while A-07 is waived/open. If this path is chosen, a governance decision record must be created and signed by project owner.
> - **Current posture:** No external tester engaged. A-07 remains OPEN. No strategy decision has been made. See `docs/13-adrs/16-solo-private-operation-waiver.md` for solo private-only waiver and self-attestation.
>
> This addendum captures the execution prerequisites and evidence checklist required to move A-07 from "planning artifact" to "closed with evidence." Self-scanning (automated tools, internal reconnaissance) is acceptable as **preparation only** and will NOT close A-07.

### Prerequisites Before Engaging External Tester

| # | Prerequisite | Owner | Status |
|---|-------------|-------|--------|
| 1 | Named external tester/vendor selected (HackerOne, Bugcrowd, or vetted freelance) | Security | 🔴 OPEN |
| 2 | Isolated staging environment provisioned (separate GCP project or isolated VPC; NOT the live production project) | SRE / Security | 🟡 DEPLOYED — SAME PROJECT | Staging clone `a07-staging-postgres-20260618143121` and namespace `intent-rebase-staging` deployed within project `ferrum-497801`. Separate GCP project or isolated VPC still recommended for full isolation before external engagement. |
| 3 | Staging environment populated with **synthetic data only** — no production credentials, no production customer data, no live API keys | SRE / Security | ✅ DONE | Synthetic data seeded via authenticated API calls into namespace `intent-rebase-staging` (2026-06-19). 2 synthetic tenants (`9a47fec7-f5e8-4fda-b676-c8f0aace455b`, `b38d546d-30f2-401e-92f6-59c12a9b5444`), 5 intents created (all 201), 5 version-2s created (all 201). Graph nodes `synthetic-graph-a` (`188c9cc3-5c6f-4b2b-af5e-473f3cfbd0ae`) and `synthetic-graph-b` (`a3124f2e-671b-4ce9-835e-126d6b2dfa2d`) created under tenant 1. Webhook subscription `3f4e81f5-9405-4cac-9f8e-897fda560728` created under tenant 1 / intent `ac55015d-6545-4fc9-94ee-0c695565fb24` with URL `https://example.invalid/intent-rebase-synthetic-webhook` (safe non-routable domain). Markers: `TENANT_COUNT 2`, `INTENT_COUNT 5`, `VERSION2_COUNT 5`, `EXTRA_SYNTHETIC_SEED_ATTEMPTED=true`. JWT token read out-of-band only; not printed or committed. No production data or credentials used. **Caveat:** Approval scenarios, audit/forensic entries, runtime-adapter mocks are not seeded; only intent/version/graph/webhook data present. |
| 4 | Separate staging credentials issued (staging API keys, staging JWT secrets, staging DB passwords) | Security | ✅ DONE | Staging DB user password rotated to separate credential; staging K8s Secret `app-secrets` created out-of-band with staging DB URL and generated JWT/API/HMAC. Secret values stored outside repo only. |
| 5 | Staging Alertmanager/Slack/SMTP channels configured for test notification (do not route to production channels) | SRE | 🔴 OPEN |
| 6 | A-04 updated security signoff obtained for any new external surface (e.g., if public ingress is created for staging) | Security | 🔴 OPEN |
| 7 | `deletion_protection = true` enabled on GKE and Cloud SQL before any external testing begins | SRE | ✅ DONE | Applied on 2026-06-18 via Terraform; GKE cluster `deletion_protection=true`, Cloud SQL deletion protection enabled. |
| 8 | Legal/contractual scope agreement signed with external tester (no production exploitation, no data destruction, no lateral movement) | Security / Legal | 🔴 OPEN |

### Evidence Checklist (Required to Close A-07)

| # | Evidence Item | Format | Verified By |
|---|-------------|--------|-------------|
| 1 | External tester engagement contract / SOW | PDF | Security |
| 2 | Executive summary report | PDF | External tester |
| 3 | Detailed findings report with CVSS scores | PDF + JSON | External tester |
| 4 | Remediation tracking spreadsheet with owner + timeline | CSV / Sheet | Security |
| 5 | All HIGH and CRITICAL findings remediated with evidence (screenshots, test output, PR links) | PDF + repo links | Security |
| 6 | Retest confirmation from external tester verifying remediations | PDF | External tester |
| 7 | Staging environment destroyed / credentials rotated after testing completes | Terraform destroy / `gcloud` logs | SRE |

### Forbidden Claims

| Forbidden Claim | Allowed Replacement |
|----------------|-------------------|
| `A-07 pen test passed` | `A-07 pen test scope defined; execution pending external engagement` |
| `A-07 closed` | `A-07 BLOCKED / PENDING — execution plan documented, no external tester engaged` |
| `Self-scan closes A-07` | `Self-scan is preparation only; A-07 requires named external tester + evidence checklist` |
