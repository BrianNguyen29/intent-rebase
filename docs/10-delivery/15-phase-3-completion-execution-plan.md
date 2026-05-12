# Phase 3 Completion Execution Plan

> **Status:** `CLOSED — Non-Production Only (2026-05-11)`
> **Phase:** Phase 3 (Post Batch 1)
> **Owner:** Backend Lead / SRE / Security
> **Last Updated:** 2026-05-11

---

## Purpose

This document provides a detailed P0/P1/P2 execution plan for completing Phase 3 and advancing toward Phase 4 entry. It formalizes decisions, captures blocked/unblocked status with explicit owners and evidence requirements, and serves as the master checklist for Phase 3 exit gate.

> **Scope Constraint:** This plan covers documentation, design approvals, and evidence collection. No Rust implementation changes are in scope. DLQ worker implementation and S3 wiring are explicitly deferred to Phase 4 (G1–G5 gates must pass first).

---

## P0 — Critical Path (Phase 2b Sign-Off Close-Out)

### P0-1: Phase 2b Sign-Off Name/Date Documentation

| Field | Value |
|-------|-------|
| **Task** | Document Phase 2b reviewer sign-off names and dates |
| **Owner** | Backend Lead |
| **Status** | ✅ RESOLVED — personal project single-signer |
| **Evidence Required** | Brian Nguyen sign-off (Product Owner, Security, Runtime Integration — single signer) |
| **Signer** | Brian Nguyen — 2026-04-28 |
| **Blocker** | None — personal project, single-signer approval documented |

**Note:** This is a personal project with Brian Nguyen acting as sole signer for all three reviewer roles (Product Owner, Security, Runtime Integration). All three sign-offs are fulfilled by the same individual.

**Action:** Updated `11-phase-2b-sign-off-packet.md` Section 5 table with Brian Nguyen / 2026-04-28 for all three roles.

**Validation:** Updated sign-off packet with Brian Nguyen as sole signer for all three roles, dated 2026-04-28.

---

## P1 — High Priority (Phase 3 Hardening)

### P1-1: Forensic S3 Decision — Option B (Default Safe Path)

| Field | Value |
|-------|-------|
| **Decision** | **Option B — DEFAULT** |
| **Status** | ✅ DECIDED (safe default) |
| **Description** | Default in-memory runtime bundle storage; S3BundleStorage is env-gated via FORENSIC_BUNDLE_STORAGE=s3. Full S3 lifecycle, Object Lock, chain-hash, and retrieval integrity deferred to Phase 4. |
| **Owner** | Backend Lead |
| **Evidence** | S3BundleStorage seam documented in `crates/forensic-service/`; env-gated instantiation via `FORENSIC_BUNDLE_STORAGE=s3` in intent-api main wiring |

**Option B Characteristics:**
- Bundle generation (`POST /forensic/bundle`) writes bundle JSON to in-memory `InMemoryBundleStorage` at runtime
- `S3BundleStorage` is env-gated (FORENSIC_BUNDLE_STORAGE=s3); not instantiated by default
- List bundles (`GET /forensic/bundles`) operates on in-memory storage
- Download (`GET /forensic/bundles/{bundle_id}/download`) operates on exportable bundles only
- S3-backed retrieval, storage lifecycle, Object Lock, chain-hash, and retention enforcement are Phase 4 scope

**Rationale for Option B as Default:**
- No lifecycle policies required on S3 buckets
- No Object Lock or chain-hash enforcement needed
- No retention period enforcement
- No S3 bucket provisioning or IAM policy configuration
- Bundle data is reconstructible from source services (Postgres, artifact store)

---

### P1-2: Option A Risk Checklist (Requires Explicit User Acknowledgement)

> **⚠️ Option A requires explicit user acknowledgement before proceeding. Do not implement Option A without completed acknowledgement.**

If Option A (wire S3BundleStorage into runtime) is desired, the following risks must be explicitly acknowledged:

| Risk Item | Description | Acknowledgement Required |
|-----------|-------------|-------------------------|
| **R-1: No Lifecycle Policies** | S3 bucket has no lifecycle rules configured. Objects will not auto-expire. | User must confirm lifecycle policy is managed externally or acknowledge indefinite retention |
| **R-2: No Object Lock** | Objects are not WORM-protected. Objects can be overwritten or deleted. | User must confirm this is acceptable for forensic integrity requirements |
| **R-3: No Chain-Hash** | Bundle integrity does not include chain-hash linking to prior bundles. | User must confirm single-bundle integrity is sufficient |
| **R-4: No Retention Enforcement** | No enforceRetention header or S3 Object Lock period is set. | User must confirm retention period is managed at application layer |
| **R-5: No Versioning Assumption** | S3 bucket versioning may not be enabled. | User must confirm versioning is enabled if point-in-time recovery is needed |
| **R-6: No Immutable Storage** | S3 objects can be mutated by any IAM principal with s3:PutObject permission. | User must confirm IAM policy restricts mutation |

**Option A Acknowledgement Template:**

```
I acknowledge the following risks of wiring S3BundleStorage without
lifecycle policies, Object Lock, chain-hash, or retention enforcement:

□ R-1: No lifecycle policies — objects will not auto-expire
□ R-2: No Object Lock — objects can be overwritten or deleted
□ R-3: No chain-hash — no cryptographic link between bundles
□ R-4: No retention enforcement — retention managed at app layer only
□ R-5: No versioning assumption — point-in-time recovery not guaranteed
□ R-6: No immutable storage — IAM principals with s3:PutObject can mutate

User Name: <user input required>
Date: <user input required>
Explicit Approval: <user input required>
```

---

### P1-3: NATS Consumer Lifecycle — Plan Only (Implementation Blocked)

| Field | Value |
|-------|-------|
| **Task** | Plan NATS consumer subscription lifecycle |
| **Status** | 🔴 BLOCKED — implementation blocked until G1, G2, G4, G5 evidence and ideally G3 stubs/metrics plan |
| **Owner** | Backend Lead / SRE |
| **In Scope** | Plan only; no Rust implementation |
| **Existing Pieces** | `NatsPullConsumerAdapter`, `JetStreamInitializer`, `NatsEventPublisher` |
| **Blocker** | G1 (design self-review) + G2 (JetStream config) + G4 (RB11 runbook) + G5 (tests) required; G3 stubs/metrics plan strongly recommended before G5 |

**Plan Components (documentation only):**

| Component | Status | Notes |
|-----------|--------|-------|
| Consumer subscription lifecycle (startup/shutdown) | 📋 Planned | Design documented in `14-dlq-retry-design.md` |
| JetStream stream configuration | 📋 Planned | Streams defined in `12-trace-propagation-blocker-matrix.md` |
| DLQ routing (dead-letter subjects) | 📋 Planned | Subject naming: `{origin_subject}.DLQ` |
| Consumer group configuration | 📋 Planned | Durable consumer names, delivery policy |
| Background worker runtime | 🔴 Blocked | Gated on G1-G5 |
| Health monitoring / consumer lag | 🔴 Blocked | Gated on G3 (DLQ metrics must exist) |

**Existing Pieces Available:**

```
crates/intent-api/src/nats_event_publisher.rs   — NatsEventPublisher (publish side)
crates/intent-rebase-types/src/event_publisher.rs — EventPublisher trait
crates/intent-api/src/nats_jetstream.rs          — NatsPullConsumerAdapter (exists, not wired)
infrastructure/local/docker-compose.yml          — NATS with JetStream (local dev)
```

**Implementation Gate Diagram:**

```
G1 (design self-review) ─────┐
                              ├──► G5 (tests) ──► Implementation
G2 (JetStream config) ────────┤     ▲
                              │     │
G4 (RB11 runbook) ────────────┤   G3 (stubs/metrics plan)
                              │   strongly recommended
G3 (DLQ metrics) ─────────────┘
```

**Validation:** Consumer lifecycle plan documented with evidence requirements. No Rust code changes.

**See Also:** [16-solo-ops-evidence-plan.md](./16-solo-ops-evidence-plan.md) — Phase A/B/C execution plan with commands to validate G2 locally.

---

### P1-4: DLQ G1–G5 Gate Evidence Checklist

| Gate | Description | Owner | Evidence Required | Status |
|------|-------------|-------|-------------------|--------|
| **G1: Design Approval** | DLQ/retry design doc reviewed and approved | Backend Lead + SRE | Approved design doc with sign-off lines | 🟢 PASS (solo self-review) — Brian Nguyen / 2026-04-28; external dual sign-off not claimed |
| **G2: JetStream Config** | JetStream streams and consumers configured with retry/advisory config | Backend Lead | nats-box stream info and consumer info showing max_deliver=3, ack_wait=30s, explicit ack, pull mode | 🟢 PASS — JetStream retry/advisory config validated via nats-box (stream/consumer with max_deliver=3) |
| **G3: Monitoring** | DLQ metrics exposed and alerting rules deployed | SRE | Prometheus metrics endpoint shows `dlq_messages_current`, `dlq_message_age_seconds`; Alertmanager has DLQ alert rules | 🔴 PENDING — G3 stubs/metrics plan required before G5 tests |
| **G4: Runbook** | DLQ investigation and replay procedure documented | SRE | RB11 in `docs/09-operations/05-runbooks.md` covering DLQ inspection, validation, replay | 🟢 PASS — RB11 present in `docs/09-operations/05-runbooks.md`; external SRE approval not claimed |
| **G5: Test Coverage** | Unit tests for retry logic, DLQ routing, replay | Backend Lead | `cargo test --all-features` passes; DLQ test suite results | 🟢 PASS (bounded) — 9 unit tests + 7 live ignored tests passed; app-level DLQ routing remains Phase 4+ future |

**DLQ Gate Evidence Template (Solo Self-Review Track):**

```
G1: Design Approval (SOLO SELF-REVIEW)
  □ Design doc reviewed by Backend Lead (solo)
  □ Self-approval: Brian Nguyen / <date>
  □ Note: SOLO SELF-REVIEW — weaker than Backend Lead + SRE dual sign-off

G2: JetStream Config (LOCAL DOCKER-COMPOSE)
  □ Streams created with DLQ subjects
  □ Evidence: docker compose exec nats nats stream ls output attached
  □ Evidence: consumer config showing dead_letter subject
  □ Note: LOCAL ONLY — not production-equivalent

G3: Monitoring Stubs/Plan (REQUIRED BEFORE G5 TESTS)
  □ dlq_messages_current metric defined in code
  □ dlq_message_age_seconds metric defined in code
  □ dlq_replay_total metric defined in code
  □ dlq_replay_failures_total metric defined in code
  □ Evidence: grep for dlq_* in crates/
  □ Note: Full G3 requires production deployment; stubs/plan required for G5

G4: Runbook
  □ DLQ investigation procedure written (RB11)
  □ DLQ replay procedure written (RB11)
  □ Self-approved by Backend Lead (solo)
  □ Evidence: RB11 present in docs/09-operations/05-runbooks.md

G5: Test Coverage (AFTER G1-G4 COMPLETE)
  □ Retry logic unit tests pass
  □ DLQ routing integration tests pass
  □ Replay procedure manual test passes
  □ Evidence: cargo test output, annotated terminal output
  □ Note: G5 tests require G3 metric stubs to exist first
```

> **⚠️ No DLQ worker code may be implemented until all five gates (G1–G5) show PASS.**
>
> **NATS consumer lifecycle implementation remains blocked until G1, G2, G4, G5 evidence exists and ideally G3 stubs/metrics plan exists.** See `16-solo-ops-evidence-plan.md` for full blocking diagram.

**See Also:** [16-solo-ops-evidence-plan.md](./16-solo-ops-evidence-plan.md) — Phase A/B/C execution plan for solo self-review with commands and evidence templates.

---

### P1-5: Production Load Testing Evidence Plan

| Field | Value |
|-------|-------|
| **Task** | Document and plan production load testing |
| **Status** | 🔴 BLOCKED — L3-L5 gated on staging/production infra |
| **Owner** | SRE / Backend Lead |
| **Bounded Slices Delivered** | HTTP load harness (L1), SQLx-backed local-live test (L2) |
| **Remaining** | Full production load testing with staging/production infra |
| **Solo Self-Review** | See [16-solo-ops-evidence-plan.md](./16-solo-ops-evidence-plan.md) Phase B-2 for L3 staging-like evidence collection |

**Load Testing Stages:**

| Stage | Scope | Tool | Owner | Status |
|-------|-------|------|-------|--------|
| **L1: Bounded HTTP Harness** | intent-api HTTP server, in-memory repos | Custom harness | Backend Lead | ✅ DELIVERED |
| **L2: SQLx Local-Live** | docker-compose Postgres, pool config | Custom harness | Backend Lead | ✅ DELIVERED |
| **L3: Staged ENV k6** | Staging environment, NATS + Postgres-like infra | k6 | SRE | 🔴 PENDING — docker-compose full stack validation available |
| **L4: Staged ENV Artillery** | Alternative to k6 for staged ENV | Artillery | SRE | 🔴 PENDING — requires staging env |
| **L5: Production Load Test** | Production environment | k6/Artillery | SRE | 🔴 BLOCKED — Phase 3 exit gate only |

> **⚠️ L1/L2 Evidence Strength:** L1/L2 results are from local docker-compose environment. Do not claim these as staging or production load test results. They demonstrate bounded test capability, not production performance.

**Required Metrics (per stage):**

| Metric | Threshold | Notes |
|--------|-----------|-------|
| p95 latency | < 100ms | Intent processing endpoint |
| p99 latency | < 250ms | Intent processing endpoint |
| Error rate | < 0.1% | 5xx errors / total requests |
| CPU utilization | < 70% | At peak load |
| Memory utilization | < 80% | At peak load |
| NATS message throughput | > 1000 msg/s | Sustained |
| Postgres connection pool | < 80% utilized | At peak load |

**Load Test Evidence Template:**

```
Stage: <L1|L2|L3|L4|L5>
Date: <timestamp>
Tool: <k6|Artillery|custom>
Environment: <in-memory|docker-compose|staging|production>

Results:
  □ Total requests: <N>
  □ Duration: <seconds>
  □ p50 latency: <ms>
  □ p95 latency: <ms>
  □ p99 latency: <ms>
  □ Error rate: <%>
  □ Throughput: <req/s>

Resource Metrics:
  □ CPU: <%>
  □ Memory: <MB used / MB total>
  □ NATS: <connections, msg/s>
  □ Postgres: <active connections, pool util %>

Validation:
  □ All thresholds met (YES/NO)
  □ Report timestamped (YES/NO)
  □ Raw output attached (YES/NO)

Evidence Strength: <local-docker-compose|staging-like|staging|production>

Solo Self-Review (if L3):
  □ Self-reviewed by Brian Nguyen
  Date: <date>
  Note: SOLO SELF-REVIEW — not equivalent to external SRE sign-off

SRE Review (if external):
  Name: _______________
  Date: _______________
  Sign-off: _______________
```

**See Also:** [16-solo-ops-evidence-plan.md](./16-solo-ops-evidence-plan.md) Phase B-2 — commands to collect L3 staging-like evidence against docker-compose full stack.

---

### P1-6: SRE/Security Sign-Off Evidence Checklist

> **Two Tracks:** This checklist supports both solo self-review (personal project) and external sign-off tracks. Solo self-review is **weaker evidence**. See [16-solo-ops-evidence-plan.md](./16-solo-ops-evidence-plan.md) for the solo self-review path.
>
> **WAIVED-SOLO Policy (Non-Production Phase 3 Only):** External gates may be marked WAIVED-SOLO for non-production Phase 3 close-out. This is valid **only for feature completion tracking** and **must be revisited with named external evidence before any production deployment or production-readiness claim**.

| Area | Item | Evidence Required | Owner | Solo Self-Review Status | External Sign-Off Status |
|------|------|-------------------|-------|------------------------|-------------------------|
| **SLOs** | SLO definitions confirmed | SRE confirms provisional SLO targets are acceptable | SRE | 🟡 Self-reviewed (solo) | 🟡 WAIVED-SOLO — requires SRE before production |
| **SLOs** | SLO dashboard available | Grafana dashboard URL or screenshot | SRE | 🟡 Self-reviewed (local) | 🟡 WAIVED-SOLO — requires prod before production |
| **SLOs** | Error budget panels active | Grafana panels showing burn rate | SRE | 🟡 Self-reviewed (local) | 🟡 WAIVED-SOLO — requires prod before production |
| **Alerting** | Alertmanager config deployed | Alertmanager config or curl output | SRE | 🟡 Config self-reviewed (local) | 🟡 WAIVED-SOLO — requires prod before production |
| **Alerting** | Alert routing confirmed | Alerts route to correct channels | SRE | 🔴 BLOCKED (requires prod) | 🟡 WAIVED-SOLO — requires prod before production |
| **Telemetry** | OTLP endpoint connected | OTEL collector receiving data | SRE | 🔴 BLOCKED (requires prod) | 🟡 WAIVED-SOLO — requires prod before production |
| **Telemetry** | Trace context propagated | W3C traceparent in logs/traces | SRE | 🟡 PARTIAL — in-process done | 🟡 WAIVED-SOLO — cross-process in progress |
| **Runbooks** | RB1–RB11 available | Runbooks in `docs/09-operations/05-runbooks.md` | SRE | ✅ Self-approved (solo) | 🟡 WAIVED-SOLO — requires SRE approval before production |
| **Load Testing** | Full production load test | Timestamped k6/Artillery report | SRE | 🔴 BLOCKED — see P1-5 | 🟡 WAIVED-SOLO — see P1-5 |
| **Pen Test** | Penetration testing executed | Pen test report (external) | Security | 🔴 BLOCKED (requires external) | 🟡 WAIVED-SOLO — requires external before production |
| **Pen Test** | Findings remediated | Evidence of fix for each finding | Security | 🔴 BLOCKED (requires pen test) | 🟡 WAIVED-SOLO — requires pen test before production |
| **External Review** | External security review sign-off | External reviewer name/date/statement | Security | 🔴 BLOCKED (requires external) | 🟡 WAIVED-SOLO — requires external before production |
| **Compliance** | SOC2/GDPR/ISO27001 checklist | Compliance checklist with all items checked | Security | 🟡 Self-reviewed (solo) | 🟡 WAIVED-SOLO — requires audit before production |
| **Incident Response** | IR plan documented and tested | IR plan doc + test evidence | SRE | 🟡 Plan self-reviewed (solo) | 🟡 WAIVED-SOLO — tabletop not run before production |
| **Failover** | Failover/recovery tested | Test results | SRE | 🔴 BLOCKED (requires prod) | 🟡 WAIVED-SOLO — requires prod before production |
| **Deployment** | Production deployment verified | Deployment runbook + verification | SRE | 🔴 BLOCKED (requires prod) | 🟡 WAIVED-SOLO — requires prod before production |

**SRE Sign-Off Evidence Template (Solo Self-Review Track):**

```
SRE Self-Review Section (Solo Practitioner)
===========================================

Evidence Strength: SOLO SELF-REVIEW — NOT equivalent to external SRE sign-off

SLO Confirmation:
  □ Provisional SLO targets self-reviewed and acceptable
  Self-Reviewer: Brian Nguyen
  Date: <date>

Alerting:
  □ Alertmanager configuration self-reviewed (local docker-compose)
  □ Alert routing verified locally
  Self-Reviewer: Brian Nguyen
  Date: <date>

Telemetry:
  □ OTLP endpoint connected (local) — self-reviewed
  □ Cross-process trace propagation (in-process done)
  Self-Reviewer: Brian Nguyen
  Date: <date>

Runbooks:
  □ RB1-RB11 self-approved
  Self-Reviewer: Brian Nguyen
  Date: <date>

Load Testing:
  □ L1/L2 local evidence self-reviewed
  □ L3-L5 staging/production blocked
  Self-Reviewer: Brian Nguyen
  Date: <date>

Failover:
  □ BLOCKED — requires production environment
```

**SRE Sign-Off Evidence Template (External Track):**

```
SRE Review Section (External)
==============================

SLO Confirmation:
  □ Provisional SLO targets reviewed and confirmed acceptable
  SRE Name: _______________
  Date: _______________

Alerting:
  □ Alertmanager configuration reviewed
  □ Alert routing verified
  SRE Name: _______________
  Date: _______________

Telemetry:
  □ OTLP endpoint connected and receiving data
  □ Cross-process trace propagation confirmed
  SRE Name: _______________
  Date: _______________

Runbooks:
  □ RB1-RB11 reviewed and approved
  SRE Name: _______________
  Date: _______________

Load Testing:
  □ Full production load test reviewed and passed
  SRE Name: _______________
  Date: _______________

Failover:
  □ Failover/recovery tested
  SRE Name: _______________
  Date: _______________

Deployment:
  □ Production deployment verified
  SRE Name: _______________
  Date: _______________
```

**Security Sign-Off Evidence Template (Solo Self-Review Track):**

```
Security Self-Review Section (Solo Practitioner)
================================================

Evidence Strength: SOLO SELF-REVIEW — NOT equivalent to external security review

Penetration Testing:
  □ Pen test scope defined and agreed (documented in 08-security/06-pen-test-scope.md)
  □ Execution BLOCKED — requires external engagement
  Self-Reviewer: Brian Nguyen
  Date: <date>

External Security Review:
  □ BLOCKED — requires external reviewer
  Self-Reviewer: Brian Nguyen
  Date: <date>

Compliance:
  □ SOC2/GDPR/ISO27001 checklist self-reviewed
  □ All items checked or exceptions documented
  Self-Reviewer: Brian Nguyen
  Date: <date>

JWT / RLS / Audit:
  □ JWT auth middleware self-reviewed
  □ RLS policies self-reviewed
  □ Audit immutability self-reviewed
  Self-Reviewer: Brian Nguyen
  Date: <date>
```

**Security Sign-Off Evidence Template (External Track):**

```
Security Review Section (External)
===================================

Penetration Testing:
  □ Pen test scope defined and agreed
  □ Pen test executed by qualified tester
  □ All HIGH/CRITICAL findings remediated
  Security Name: _______________
  Date: _______________

External Security Review:
  □ External reviewer engaged
  □ Review completed with no blocking findings
  Security Name: _______________
  Date: _______________

Compliance:
  □ SOC2/GDPR/ISO27001 checklist reviewed
  □ All items checked or exceptions documented
  Security Name: _______________
  Date: _______________

JWT / RLS / Audit:
  □ JWT auth middleware reviewed
  □ RLS policies reviewed
  □ Audit immutability reviewed
  Security Name: _______________
  Date: _______________
---

## P2 — Phase 4 Backlog (Gated on Phase 3 Exit)

### P2-1: Phase 4 Entry Criteria

| Criterion | Owner | Status |
|-----------|-------|--------|
| Phase 3 Batch 1 closed | Backend Lead | ✅ Done |
| Phase 3 Batch 2 closed (SRE/Observability) | SRE | 🟡 In Progress |
| Phase 3 Batch 3a closed (Tenant Isolation) | Security | 🟡 In Progress |
| Phase 3 Batch 3b closed (Forensic Bundle) | Backend Lead | 🟡 In Progress |
| Phase 3 Batch 4a closed (Performance) | Backend Lead / SRE | 🟡 In Progress |
| Phase 3 Batch 4b closed (Security) | Security | 🟡 In Progress |
| Phase 3 exit gate closed | All | ✅ CLOSED — Non-Production Only (2026-05-11, Brian Nguyen / Backend Lead solo) |

---

### P2-2: Phase 4 Scope Items (Deferred)

| Item | Phase 4 Scope | Owner | Blocker |
|------|-------------|-------|---------|
| **Forensic Full Replay** | Replay bundle to reproduce system state | Backend Lead | Phase 3 Batch 3b must close |
| **S3 Object Lock/Retention** | WORM-protected forensic bundles | Backend Lead | Option A acknowledgement required (if wired) |
| **S3 Chain-Hash** | Cryptographic linking between bundles | Backend Lead | Option A acknowledgement required (if wired) |
| **S3 Retrieval Integrity** | Verify bundle integrity on S3 retrieval | Backend Lead | Option A acknowledgement required (if wired) |
| **Temporal/sqlx Propagation** | Cross-process trace propagation via workflow payload or sqlx tags | Backend Lead / SRE | B-01 / B-02 decisions |
| **Deployment/CD Hardening** | GitOps, progressive rollout, rollback automation | SRE | Phase 3 exit gate |
| **Ops Hardening** | Tenant onboarding/offboarding automation | SRE | Phase 3 exit gate |
| **DLQ Worker Implementation** | Automatic DLQ replay worker | Backend Lead | G1–G5 must pass |
| **NATS Consumer Lifecycle** | Full consumer wiring with background worker | Backend Lead | G1–G5 must pass |
| **S3BundleStorage Wiring** | Wire S3 storage into forensic-service runtime | Backend Lead | Option A acknowledgement required |

---

### P2-3: Phase 4 Entry Execution Notes

**Before Phase 4 begins:**

1. All Phase 3 Batch items must show CLOSED status in `09-completion-proposals-tracker.md`
2. Phase 3 exit gate must show CLOSED in `00-current-status.md`
3. All P1 blockers in this document must show PASS or explicit user acknowledgement
4. DLQ G1–G5 gates must show PASS before any DLQ worker or NATS consumer implementation begins

**Phase 4 priority ordering (suggested):**

| Priority | Item | Rationale |
|----------|------|-----------|
| P1 | Forensic Full Replay | Highest user value; existing seam |
| P1 | DLQ Worker (after G1–G5 pass) | Required for production reliability |
| P1 | S3 Object Lock/Retention (if Option A) | Compliance requirement |
| P2 | Temporal/sqlx Propagation | Engineering efficiency |
| P2 | Deployment/CD Hardening | Operational efficiency |
| P3 | Tenant Onboarding Automation | Operational efficiency |

---

## Master Evidence Checklist

| ID | Item | Owner | Evidence | Status |
|----|------|-------|----------|--------|
| E-01 | Phase 2b sign-off names/dates | Backend Lead | Updated sign-off packet | ✅ RESOLVED (Brian Nguyen / 2026-04-28, single-signer) |
| E-02 | S3 decision (Option B default) | Backend Lead | This document | ✅ DECIDED |
| E-03 | Option A risk acknowledgement (if chosen) | User | Signed acknowledgement form | 🔴 BLOCKED (if Option A desired) |
| E-04 | DLQ G1–G5 gate evidence | Backend Lead / SRE | Gate checklist with signatures | 🟡 PARTIAL/PASS (bounded solo evidence) — G1 solo, G2 validated, G3 stubs, G4 RB11, G5 bounded tests; external sign-off/app-level DLQ routing not claimed |
| E-05 | Production load test report | SRE | Timestamped k6/Artillery report | 🟡 WAIVED-SOLO (non-production Phase 3 only) |
| E-06 | SRE sign-off | SRE | Signed SRE checklist | 🟡 WAIVED-SOLO (non-production Phase 3 only) |
| E-07 | Security sign-off | Security | Signed security checklist | 🟡 WAIVED-SOLO (non-production Phase 3 only) |
| E-08 | Phase 3 exit gate closed | All | Gate status updated | ✅ CLOSED — Non-Production Only (2026-05-11, Brian Nguyen / Backend Lead solo); WAIVED-SOLO external gates accepted for non-production Phase 3 only; must close with named external evidence before production claim |

---

## Phase 3 Completion Todo List

> **Purpose:** Single explicit todo-list for Phase 3 completion. All items must show STOP (gate closed) before Phase 4 begins. External items are owner-labeled but require external engagement to close.

| # | Priority | Item | Owner | Status | Evidence | Next Action | Stop/Go Criteria |
|---|----------|------|-------|--------|----------|-------------|------------------|
| T-01 | P0 | Phase 3 exit gate | All | ✅ CLOSED — Non-Production Only (2026-05-11, Brian Nguyen / Backend Lead solo) | Gate status in `checklist-phase-3.md` | Gate administratively closed with WAIVED-SOLO documented for all external gates | GO: Closed for non-production Phase 3 feature completion; must revisit with named external evidence before production claim |
| T-02 | P1 | External SRE sign-off | SRE | 🟡 WAIVED-SOLO (non-production Phase 3 only) | SRE sign-off packet in `10-external-review-packet.md` | Prepare SRE review packet; engage external SRE | GO: External SRE name/date/statement recorded before production claim |
| T-03 | P1 | External security review | Security | 🟡 WAIVED-SOLO (non-production Phase 3 only) | Security sign-off packet in `10-external-review-packet.md` | Prepare security review packet; engage external reviewer | GO: External security reviewer name/date/statement recorded before production claim |
| T-04 | P1 | Production infrastructure | SRE | 🟡 WAIVED-SOLO (non-production Phase 3 only) | docker-compose local only | Requires production env provisioning | GO: Production env verified operational before deployment |
| T-05 | P1 | Load testing L3–L5 | SRE | 🟡 WAIVED-SOLO (non-production Phase 3 only) | L1/L2 results in `docs/11-quality/load-test-results.md` | Requires staging/production infra | GO: L3 staging-like results; GO when L4/L5 pass before production claim |
| T-06 | P1 | Penetration testing | Security | 🟡 WAIVED-SOLO (non-production Phase 3 only) | Threat model v2 in `docs/14-governance/06-threat-model-v2.md`; pen scope in `docs/08-security/06-pen-test-scope.md` | Engage external pen test team | GO: External pen test report; no blocking findings before production claim |
| T-07 | P1 | Artifact side-effect tx boundary design-first | Backend Lead | ✅ DESIGN NOTE | ADR-08 in `docs/13-adrs/08-artifact-side-effect-tx-boundary.md` | None — design complete | GO: Design-first approach followed; implementation Phase 4+ |
| T-08 | P1 | SqlxBundleRepository + forensic bundle RLS wiring (RLC-13) | Backend Lead | ✅ BOUNDED VERIFIED | `infrastructure/migrations/016_create_forensic_bundles.sql`; `crates/forensic-service/src/bundle_repo.rs`; `crates/intent-api/tests/rls_integration.rs` | SQL-backed forensic bundle repo, migration 016, runtime SQL wiring, and RLC-13 tenant isolation test delivered | GO: Bounded local slice verified; targeted live RLC-13 passed on isolated local Postgres |
| T-09 | P2 | OpenAPI batch-execute RLS semantics documentation | Backend Lead | ✅ DOCUMENTED | `docs/04-api/openapi.yaml` (batch-execute description updated) | OpenAPI spec updated with per-item RLS tx semantics | GO: OpenAPI spec updated |
| T-10 | P2 | rebase_apply handler review | Backend Lead | ✅ DESIGN RESOLVED — ADR-09 accepted; bounded Slice 1/2 graph RLS seam + post-hoc check delivered | `docs/13-adrs/09-rebase-apply-rls-transaction-boundary.md`; `crates/intent-api/src/rebase_apply_handlers.rs`; `crates/intent-service/src/approval_request_repo.rs`; `crates/graph-service/src/sqlx_graph_repository.rs` | Bounded `BlockedManualReview` approval create/cancel RLS slice verified. Bounded graph RLS slice delivered: `SqlxGraphRepository::update_node_state_with_tx` added; JWT AutoProceeded/AutoProceededWithNotification post-hoc RLS tx check/update applied after successful graph updates; fallback preserved when no RLS pool/claims/SQL repo. ADR-09 records accepted three-phase design (read-only tx → write tx → post-commit signal) with caller-side orchestration and non-RLS fallback. Remaining implementation (D1–D7) is Phase 4 scope. | GO: Design resolved per ADR-09; D1–D7 deferred to Phase 4; external SRE/security/load/pen still blocked |
| T-11 | P0 | No-CI posture maintained | Backend Lead | ✅ INTENTIONAL | GitHub Actions disabled; local gates are source of truth | None | GO: CI remains disabled; no CI-green claims |

**Gate Closure Rule:** Phase 3 gate closes when T-01 shows GO (gate sign-offs obtained or WAIVED-SOLO documented) AND all P1 items (T-02 through T-08) show either GO, explicit user acknowledgement, or WAIVED-SOLO with documented rationale. T-09 and T-10 are P2 — can remain open into Phase 4 with owner tracking.

**WAIVED-SOLO Policy:** T-02 through T-06 may be marked WAIVED-SOLO for non-production Phase 3 close-out only. This is valid for feature completion tracking and does not constitute production readiness. All WAIVED-SOLO items must be revisited and closed with named external evidence before any production deployment or production-readiness claim.

**External Dependencies Note:** T-02, T-03, T-04, T-05, T-06 require external engagement for production readiness. Solo self-review (WAIVED-SOLO) is accepted only for non-production Phase 3 close-out.

**See Also:**
- [checklist-phase-3.md](./checklists/checklist-phase-3.md) — Phase 3 exit gate with batch deliverable status
- [16-solo-ops-evidence-plan.md](./16-solo-ops-evidence-plan.md) — Solo self-review evidence templates
- [10-external-review-packet.md](../09-operations/10-external-review-packet.md) — SRE/security review packet templates
- [11-pen-load-test-packet.md](../09-operations/11-pen-load-test-packet.md) — Pen/load test execution packet templates

---

## Document Wiring

This document is linked from:

| Doc | Relationship |
|-----|-------------|
| `00-current-status.md` | Linked from Prioritized Next Steps table |
| `09-completion-proposals-tracker.md` | Linked from P1/P2 sections |
| `11-phase-2b-sign-off-packet.md` | P0-1 references sign-off packet for name/date fields |
| `16-solo-ops-evidence-plan.md` | Referenced in P1-3, P1-4, P1-5, P1-6; solo self-review plan |

---

## Forbidden Claims

The following must NOT appear in any Phase 3 documentation:

- `production-ready` (use: "non-production feature completion")
- `remote CI passed` (use: "local canonical gates are the required source of truth")
- `S3BundleStorage unconditionally wired` (use: "S3BundleStorage env-gated; default in-memory")
- `DLQ worker implemented` (use: "design approved; implementation gated on G1–G5")
- `NATS consumer lifecycle implemented` (use: "adapter exists; lifecycle blocked on G1-G5")
- `G1-G5 externally approved` (use: "G1 self-reviewed; G2-G5 pending")
- `production load test passed` (use: "L1/L2 local evidence exists; L3-L5 pending")
- `SRE sign-off complete` (use: "solo self-review completed; external sign-off pending")
- `Security sign-off complete` (use: "solo self-review completed; external review pending")
- `staging environment` when referring to docker-compose (use: "docker-compose local (staging-like)")
- Any real sign-off name or date without user/org input

---

## Update Log

| Date | Updated By | Changes |
|------|------------|---------|
| April 2026 | (orchestrator) | Initial creation — P0/P1/P2 execution plan, S3 decision, DLQ gates, load test plan, SRE/security checklist |
