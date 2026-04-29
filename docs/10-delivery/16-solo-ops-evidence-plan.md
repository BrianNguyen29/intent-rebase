# 16 — Solo Ops Evidence Plan

**Status:** `IN PROGRESS — Planning Document`
**Phase:** Phase 3 — Solo Self-Review Evidence Track
**Owner:** Backend Lead (solo practitioner)
**Last Updated:** April 2026

---

## Purpose

This document captures the solo self-review execution plan for DLQ G1–G5 gates, NATS consumer lifecycle planning, staging/load evidence collection, and SRE/Security self-review. It provides an actionable path toward evidence collection for a personal project where external SRE/Security sign-off is deferred but operational rigor is maintained.

> **⚠️ Evidence Strength Disclaimer**
>
> This document describes a **solo self-review track** for a personal project. Solo self-review is weaker evidence than external SRE/Security sign-off. Claims made here are **not equivalent** to external verification. Do not represent solo self-review as external SRE/Security approval.

---

## Phase A — Immediate Actions (Self-Sign G1, Write G4 Runbook, Create G5 Plan, Self-Review SRE Checklist)

### A-1: Self-Sign DLQ Design (G1 Equivalent)

| Field | Value |
|-------|-------|
| **Task** | Self-review and self-sign DLQ/retry design doc |
| **Status** | ✅ COMPLETED — self-reviewed by Brian Nguyen on 2026-04-28 |
| **Owner** | Backend Lead (solo) |
| **Evidence Required** | Signed design doc with self-approval statement |
| **Constraints** | This is solo self-review — weaker than Backend Lead + SRE dual sign-off |

**Self-Approval Statement Template:**

```
DLQ Design Self-Review (G1 Equivalent — Solo Practitioner)
============================================================

I have reviewed the DLQ/retry design in `14-dlq-retry-design.md` and confirm:

✅ DLQ subject naming convention reviewed
✅ Max redeliveries policy reviewed
✅ Dead-letter routing rules reviewed
✅ Manual replay policy reviewed
✅ Monitoring and alerting strategy reviewed

Self-Approval: Brian Nguyen — 2026-04-28
Evidence Strength: SOLO SELF-REVIEW (weaker than Backend Lead + SRE dual sign-off)
```

**Validation Command:**
```bash
# Verify design doc exists and is non-empty
wc -l docs/10-delivery/14-dlq-retry-design.md
# Expected: > 0 lines
```

---

### A-2: Write DLQ Investigation Runbook (RB11)

| Field | Value |
|-------|-------|
| **Task** | Write RB11 DLQ investigation and replay runbook |
| **Status** | ✅ COMPLETED — RB11 present in `docs/09-operations/05-runbooks.md` |
| **Owner** | Backend Lead (solo) |
| **Location** | `docs/09-operations/05-runbooks.md` (RB11 section) OR this document |
| **Evidence Required** | Completed runbook with diagnosis/mitigation/recovery steps |

**RB11 Template — DLQ Investigation and Replay:**

```
## RB11. DLQ Messages Found

**Symptoms:**
- DLQ depth alert fires (> 10 messages)
- `nats consumer ls` shows messages in DLQ subjects
- Application logs show repeated delivery failures

**Diagnosis:**
1. Inspect DLQ subject count:
   ```bash
   nats stream ls
   nats consumer ls audit_events
   ```
   2. Pull sample DLQ message to inspect:
   ```bash
   nats consumer next "audit_events" --subject audit.events.v1.approval.events.DLQ --json
   ```
3. Check message headers for `Nats-Deliver-Count`, `Nats-Message-Name`
4. Check application logs for the `Nats-Message-Name` correlation

**Mitigation:**
1. If transient failure: replay immediately
   ```bash
   nats pub audit.events.v1.approval.events --header "Nats-Replay: true" <payload.json>
   ```
2. If bug in consumer: fix consumer code FIRST, then replay
3. If poison message (malformed): do NOT replay until payload fixed
4. If downstream outage: wait for downstream recovery

**Recovery:**
1. Monitor DLQ depth: `nats stream ls` should show 0 after replay
2. Watch consumer lag: `nats consumer ls audit_events`
3. Verify no new DLQ messages appearing
4. Check application metrics in Grafana

**Prevention:**
- Monitor `dlq_messages_current` metric (alert threshold: > 10)
- Monitor `dlq_message_age_seconds` (alert threshold: > 3600s)
- Review DLQ messages weekly if volume is non-zero
```

**Validation Command:**
```bash
# Verify runbook section exists
grep -n "RB11" docs/09-operations/05-runbooks.md
# Expected: line number where RB11 is defined
```

---

### A-3: Create G5 Bounded Test Plan

| Field | Value |
|-------|-------|
| **Task** | Create bounded test plan for DLQ retry logic, routing, replay |
| **Status** | ✅ COMPLETED — bounded unit/live test plan and evidence recorded |
| **Owner** | Backend Lead (solo) |
| **Evidence Required** | Test plan document with commands to run |

**G5 Bounded Test Plan Template:**

```
DLQ G5 Test Coverage Plan (Bounded)
====================================

Test Categories:

T1: Retry Logic Unit Tests
---------------------------
Scope: Test max_deliver enforcement, backoff timing, ACK/NACK behavior
Tool: cargo test
Evidence: Test output showing PASS for dlq::retry tests

T2: DLQ Routing Integration Tests
---------------------------------
Scope: Test dead-letter subject routing when max_deliver reached
Tool: cargo test with in-memory NATS mock
Evidence: Test output showing DLQ routing works

T3: Replay Procedure Tests
---------------------------
Scope: Test replay from DLQ to original subject
Tool: Manual test against docker-compose NATS
Commands:
  # Start docker-compose NATS
  cd infrastructure/local && docker-compose up -d nats
  
  # Verify DLQ subjects configured
   nats stream ls
   nats consumer ls audit_events

   # Manually trigger DLQ (simulate max_deliver)
   # ... (test commands)

   # Verify replay works
   nats consumer next "audit_events" --subject audit.events.v1.approval.events.DLQ --no-ack

Evidence: Annotated terminal output showing successful replay

Constraints:
- No DLQ worker implementation (gated on G1-G5 pass)
- Tests verify design, not implementation
- Full test suite requires G1-G4 pass first
```

**Validation Command:**
```bash
# Check if DLQ tests exist
cargo test --all-features -- --list | grep -i dlq
# Expected: list of DLQ test names (may be empty if not yet written)
```

---

### A-4: Self-Review SRE/Security Checklist

| Field | Value |
|-------|-------|
| **Task** | Self-review SRE and security checklist items |
| **Status** | 🔴 PENDING |
| **Owner** | Backend Lead (solo) |
| **Evidence Required** | Annotated checklist with self-review notes |

**Solo Self-Review vs External Signoff Track:**

| Area | Solo Self-Review | External Signoff (Phase C) |
|------|------------------|---------------------------|
| **SLOs** | Confirm provisional SLO targets are acceptable (solo) | SRE confirms SLO targets with production data |
| **Alerting** | Review Alertmanager config for correctness (solo) | Alertmanager deployed and routing verified |
| **Runbooks** | Self-approve RB1-RB11 (solo) | SRE reviews and approves runbooks |
| **Load Testing** | Confirm L1/L2 local evidence is valid (solo) | Full production load test (L5) by SRE |
| **Pen Test** | Acknowledge pen test is pending (solo) | External pen test engagement |
| **Security Review** | Acknowledge external review is pending (solo) | External security reviewer sign-off |

**Self-Review Statement Template:**

```
SRE/Security Self-Review (Solo Practitioner)
=============================================

I have reviewed the following items and confirm they are in acceptable state
for a personal project with solo self-review:

SLOs:
□ Provisional SLO targets in docs/09-operations/04-sre-and-slos.md are documented
□ Error budget panels are active in Grafana (local)

Alerting:
□ Alertmanager config in infrastructure/local/alertmanager/alertmanager.yml
□ Prometheus rules in infrastructure/local/prometheus/rules/

Runbooks:
□ RB1-RB10 present in docs/09-operations/05-runbooks.md
□ RB11 (DLQ) added in this plan

Load Testing:
□ L1/L2 evidence in docs/11-quality/load-test-results.md
□ L3-L5 pending staging/production environment

Pen Test:
□ Threat model v2 documented in docs/08-security/06-threat-model-v2.md
□ Pen test scope defined; execution pending

Self-Review Date: <date>
Self-Reviewer: Brian Nguyen (solo practitioner)
Evidence Strength: SOLO SELF-REVIEW — not equivalent to external SRE/Security sign-off
```

---

## Phase B — Docker-Compose Validation (Requires NATS/Postgres/MinIO)

> **Note:** Phase B requires `docker compose -f infrastructure/local/docker-compose.yml up -d` to be running.

### B-1: Validate G2 JetStream Retry/Advisory Config

| Field | Value |
|-------|-------|
| **Task** | Validate JetStream stream/consumer retry/advisory config via nats-box |
| **Status** | 🟢 PASS (2026-04-28) |
| **Owner** | Backend Lead (solo) |
| **Prerequisites** | docker-compose with NATS running; nats-box available |
| **Note** | G2 validates retry/advisory config only — DLQ publishing is application-level future worker behavior |

> **⚠️ Important: No Native Automatic DLQ Routing**
>
> JetStream/async-nats does **not** have native automatic dead-letter routing in current
> Rust consumer config. The `dead_letter` field is CLI/server-side only. DLQ publishing
> is an application-level future worker behavior (Phase 4+), not a native JetStream feature.

**G2 Pass Criteria (Retry/Advisory Config):**

| Criterion | Required | Verified |
|-----------|----------|----------|
| Stream `audit_events` exists | Yes | ✅ |
| Subject filter `audit.events.v1.>` | Yes | ✅ |
| Consumer `audit_events_consumer` exists | Yes | ✅ |
| Pull consumer | Yes | ✅ |
| Ack policy explicit | Yes | ✅ |
| `max_deliver=3` | Yes | ✅ |
| `ack_wait=30s` | Yes | ✅ |
| Filter subject under `audit.events.v1.*` | Yes | ✅ |

**Validation Commands:**

```bash
# Start docker-compose
cd infrastructure/local && docker-compose up -d nats

# Verify NATS is running
docker compose -f infrastructure/local/docker-compose.yml ps nats

# Check JetStream streams via nats-box
docker run --rm --network local_default natsio/nats-box:latest nats \
  --server nats://nats:4222 stream ls

# Check consumer config
docker run --rm --network local_default natsio/nats-box:latest nats \
  --server nats://nats:4222 consumer info audit_events audit_events_consumer
```

> **Note:** The bounded stream is `audit_events` with subject filter `audit.events.v1.>`.
> The `ire.*` naming convention used in older docs is legacy and not current.

---

### B-1 Evidence: G2 JetStream Retry/Advisory Config Validation (2026-04-28)

**G2 Status: 🟢 PASS — JetStream retry/advisory config validated**

**Commands Run:**
```bash
# Start NATS
docker compose -f infrastructure/local/docker-compose.yml up -d nats

# Verify JetStream is active
docker run --rm --network local_default natsio/nats-box:latest nats \
  --server nats://nats:4222 server check jetstream
# Output: OK JetStream | ... streams=0 consumers=0 ...

# Create audit_events stream
docker run --rm --network local_default natsio/nats-box:latest nats \
  --server nats://nats:4222 stream add audit_events \
  --subjects "audit.events.v1.>" --storage file \
  --retention limits --discard old --max-age 24h --defaults
# Result: Stream audit_events was created

# Create audit_events_consumer
docker run --rm --network local_default natsio/nats-box:latest nats \
  --server nats://nats:4222 consumer add audit_events audit_events_consumer \
  --filter "audit.events.v1.>" --ack explicit --pull \
  --max-deliver 3 --wait 30s --defaults
# Result: Consumer audit_events_consumer created

# Verify stream info
docker run --rm --network local_default natsio/nats-box:latest nats \
  --server nats://nats:4222 stream info audit_events
# Output: Subjects: audit.events.v1.> ...

# Verify consumer info
docker run --rm --network local_default natsio/nats-box:latest nats \
  --server nats://nats:4222 consumer info audit_events audit_events_consumer
# Output: Maximum Deliveries: 3, Ack Wait: 30.00s, Pull Mode: true, Ack Policy: Explicit
```

**Evidence Collected:**
| Check | Result |
|-------|--------|
| NATS container started | ✅ Container intent-rebase-nats running |
| JetStream enabled | ✅ OK JetStream |
| Stream `audit_events` exists | ✅ |
| Subject filter `audit.events.v1.>` | ✅ |
| Consumer `audit_events_consumer` exists | ✅ |
| Pull consumer | ✅ |
| Ack policy explicit | ✅ |
| `max_deliver=3` | ✅ |
| `ack_wait=30s` | ✅ |
| Filter subject `audit.events.v1.>` | ✅ |

**G2 Gate Status: PASSED — JetStream retry/advisory config validated via nats-box**

> **Note:** G2 validates only stream/consumer retry configuration. It does **not** validate
> DLQ publishing — that is an application-level future worker behavior (Phase 4+).
> JetStream/async-nats has no native automatic dead-letter routing in current Rust config.

---

### G5 Evidence: Live Bounded Tests (2026-04-29)

**G5 Status: 🟢 PASSED (bounded live tests) — Full app-level DLQ routing still future**

**Evidence:**

| Check | Result |
|-------|--------|
| Live ignored tests compile | ✅ |
| Live ignored tests pass (7/7) | ✅ |
| Subject overlap issue fixed | ✅ (isolated `test.g5live.v1.*` subjects) |
| `NatsPullConsumerAdapter` max_deliver aligned | ✅ (changed from 1 to 3) |
| G2 stream `audit_events` preserved | ✅ (no overlap after fix) |

**Commands Run:**
```bash
# Run live ignored tests (requires NATS running via docker-compose)
cargo test -p intent-api --all-features --lib -- nats_jetstream::live_integration_tests --ignored

# Result: 7 passed; 0 failed; 0 ignored (live tests only)
# Finished in 0.59s
```

**Live Tests with Isolated Namespaces:**
- `live_jetstream_g5_stream_config` — verifies G2 stream exists (uses `audit_events`)
- `live_jetstream_g5_consumer_max_deliver_3` — verifies max_deliver=3 consumer config
- `live_jetstream_g5_failed_no_dlq` — documents no native automatic DLQ routing
- `live_jetstream_stream_publish_consume_ack_trace_roundtrip` — uses `test.g5live.v1.roundtrip.>`
- `live_jetstream_stream_idempotent_create` — uses `test.g5live.v1.idempotent.>`
- `live_jetstream_message_without_traceparent` — uses `test.g5live.v1.notrace.>`
- `live_jetstream_malformed_traceparent` — no stream needed (unit test)

**Bounded Unit Tests (always run):**
- `test_jetstream_initializer_default_stream_name` — ✅
- `test_jetstream_initializer_default_subject_filter` — ✅
- `test_jetstream_initializer_custom_settings` — ✅
- `test_bounded_retry_config_max_deliver_3` — ✅
- `test_bounded_ack_does_not_imply_dlq_publish` — ✅
- `test_extract_trace_context_*` (5 tests) — ✅

**G5 Gate Status: PASSED (bounded live + unit tests) — App-level DLQ routing remains future work**

> **Note:** G5 validates bounded retry config tests and live test evidence. It does **not**
> validate DLQ publishing — that is an application-level future worker behavior (Phase 4+).
> Consumer lifecycle and DLQ routing remain blocked on G1-G5 complete pass.

---

### B-2: Staging-Like Load Evidence Collection

| Field | Value |
|-------|-------|
| **Task** | Collect L3 staging-like evidence with docker-compose |
| **Status** | 🔴 PENDING |
| **Owner** | Backend Lead (solo) |
| **Prerequisites** | docker-compose full stack running |
| **Tools** | k6 or custom harness |

**Staging Evidence Plan:**

| Stage | Scope | Environment | Status |
|-------|-------|-------------|--------|
| L1 | HTTP harness, in-memory | Local binary | ✅ DELIVERED |
| L2 | SQLx-backed, docker-compose Postgres | Local docker | ✅ DELIVERED |
| L3 | Full stack, NATS + Postgres | docker-compose (staging-like) | 🔴 PENDING |
| L4 | Full stack with observability | docker-compose + Prometheus | 🔴 PENDING |
| L5 | Production load | Production infra | 🔴 BLOCKED |

**L3 Staging Evidence Commands:**

```bash
# Start full docker-compose stack
cd infrastructure/local && docker-compose up -d

# Verify all services healthy
docker compose -f infrastructure/local/docker-compose.yml ps

# Run intent-api server (in background)
cargo run -p intent-api &
sleep 5

# Run L3 load test (staging-like)
cargo test -p intent-api --features load-test --test load_test -- --nocapture test_load

# Capture metrics from Prometheus
curl -s http://localhost:9090/api/v1/query?query=intent_api_requests_total
curl -s http://localhost:9090/api/v1/query?query=intent_api_request_duration_seconds

# Evidence: attach test output, metrics output
```

**L3 Evidence Template:**
```
Stage: L3 (Staging-Like)
Date: <timestamp>
Environment: docker-compose full stack
Command: cargo test -p intent-api --features load-test --test load_test

Results:
  Total Requests: <N>
  p50 Latency: <ms>
  p95 Latency: <ms>
  p99 Latency: <ms>
  Error Rate: <%>

SLO Compliance:
  p95 < 100ms: PASS/FAIL
  Error Rate < 0.1%: PASS/FAIL

Evidence Strength: LOCAL DOCKER-COMPOSE (staging-like, not production-equivalent)
```

---

## Phase C — Deferred (Production/External Items)

The following items require production infrastructure or external engagement and are **not in scope** for solo self-review:

| Item | Reason Deferred | External Requirement |
|------|---------------|---------------------|
| Full production load test (L5) | Requires production infra | k6/Artillery report by SRE |
| Production Alertmanager deployment | Requires production environment | Alertmanager config by SRE |
| Penetration testing | Requires external engagement | Pen test report |
| External security review | Requires external reviewer | Signed statement |
| Failover/recovery testing | Requires production env | Test results by SRE |
| Production deployment | Requires deployment window | Runbook + verification |

---

## NATS Consumer Lifecycle — Blocked Until Evidence Gates

> **⚠️ Implementation Blocked**
>
> NATS consumer lifecycle implementation (background worker, subscription management) is **blocked** until the following gates show PASS:
>
> - **G1**: DLQ design self-reviewed by Brian Nguyen on 2026-04-28 (Phase A) — PASS
> - **G2**: JetStream retry/advisory config validated with stream/consumer via nats-box (Phase B) — PASS
> - **G3**: DLQ metrics stubs exist (`dlq_messages_current`, `dlq_message_age_seconds`, `dlq_replay_total`, `dlq_replay_failures_total`) — stubs compile; runtime emissions await lifecycle/worker
> - **G4**: RB11 DLQ runbook written in `docs/09-operations/05-runbooks.md` (Phase A) — PASS
> - **G5**: Bounded unit/live tests pass; app-level DLQ publish remains Phase 4+ future — PASS (bounded)
>
> **Current state:** `NatsPullConsumerAdapter` exists in `crates/intent-api/src/nats_jetstream.rs` but is **not wired** into startup/background lifecycle. DLQ metric stubs compile, but runtime DLQ metric emissions are not wired. No DLQ worker implementation exists.

**Blocked Implementation Components:**

| Component | Blocker | Status |
|-----------|---------|--------|
| Consumer subscription lifecycle | G1, G2 | 🔴 BLOCKED |
| Background worker runtime | G1-G5 | 🔴 BLOCKED |
| DLQ metrics instrumentation | G3 | 🟡 STUBS COMPILE — runtime emissions await DLQ worker/lifecycle wiring |
| DLQ alerting rules deployment | G3 | 🔴 BLOCKED — alert rules cannot be validated until runtime emissions exist |
| Automatic DLQ replay | G1-G5 | 🔴 BLOCKED |

**Evidence Required Before Implementation:**

```
NATS Consumer Lifecycle Evidence Gates
=======================================

G1: Design Approval (SOLO SELF-REVIEW) — PASS
  ✅ DLQ design doc self-reviewed
  ✅ Self-approval: Brian Nguyen / 2026-04-28
  ✅ Evidence: self-approval statement recorded in this document

G2: JetStream Retry/Advisory Config (LOCAL VALIDATION) — PASS
  ✅ Stream `audit_events` created with subject `audit.events.v1.>`
  ✅ Consumer `audit_events_consumer` created with max_deliver=3, ack_wait=30s, explicit ack, pull mode
  ✅ Evidence: nats-box stream info and consumer info output captured
  ✅ Note: JetStream/async-nats has no native automatic dead-letter routing (CLI/server-side only)
  ✅ Note: DLQ publishing is application-level future worker behavior (Phase 4+)

G3: Monitoring Stubs/Plan (REQUIRED BEFORE G5 TESTS) — STUBS COMPILE
  ✅ dlq_messages_current metric defined in code
  ✅ dlq_message_age_seconds metric defined in code
  ✅ dlq_replay_total metric defined in code
  ✅ dlq_replay_failures_total metric defined in code
  ✅ Evidence: `cargo check` / `cargo clippy -D warnings` pass; runtime emissions await lifecycle/worker

G4: Runbook Written — PASS
  ✅ RB11 DLQ investigation/replay in docs/09-operations/05-runbooks.md
  ✅ Evidence: grep RB11 docs/09-operations/05-runbooks.md

G5: Test Coverage (BOUNDED LIVE + UNIT TESTS — PASS)
  ✅ Retry logic unit tests pass (bounded nats tests)
  ✅ Live bounded tests pass (7/7 ignored tests)
  ✅ Evidence: cargo test output (live tests 7 passed; default nats tests 13 passed, 7 ignored)
  ✅ Note: Bounded tests passed; full app-level DLQ routing remains Phase 4+ future
```

---

## Forbidden Claims (Extended)

The following claims must **NOT** appear in any Phase 3 documentation:

| Forbidden Claim | Allowed Replacement |
|----------------|-------------------|
| `G1-G5 externally approved` | `G1 self-reviewed (solo); G2 retry/advisory config validated; G3 stubs compile; G4 RB11 present; G5 bounded tests pass; external approval still not claimed` |
| `production load test passed` | `L1/L2 local evidence exists; L3-L5 pending` |
| `SRE sign-off complete` | `SRE self-review (solo) completed; external sign-off pending` |
| `Security sign-off complete` | `Security self-review (solo) completed; external review pending` |
| `DLQ worker implemented` | `DLQ design approved (gated); worker implementation blocked on G1-G5` |
| `NATS consumer lifecycle implemented` | `NATS adapter exists; consumer lifecycle blocked on G1-G5` |
| `production-ready` | `non-production feature completion` |
| `remote CI passed` | `local canonical gates pass` |
| `staging environment` (when referring to docker-compose) | `docker-compose local environment (staging-like, not staging-prod)` |

---

## Document Wiring

This document is linked from:

| Doc | Relationship |
|-----|-------------|
| `15-phase-3-completion-execution-plan.md` | Referenced in P1-3, P1-4, P1-5, P1-6 |
| `sre-approval-checklist.md` | Referenced in self-review section |
| `09-operations/05-runbooks.md` | RB11 linked from this doc |

---

## Update Log

| Date | Updated By | Changes |
|------|------------|---------|
| April 2026 | (orchestrator) | Initial creation — solo self-review plan, Phase A/B/C structure, NATS lifecycle blocked gates, extended forbidden claims |
| April 2026 | (fixer) | G2 PASS: JetStream retry/advisory config validated via nats-box (stream/consumer with max_deliver=3); G5 bounded live tests evidence added (7/7 passed); NatsPullConsumerAdapter max_deliver aligned from 1 to 3; subject overlap fix (isolated test.g5live.v1.* namespaces); G5 gate marked PASS for bounded tests only; DLQ publishing remains Phase 4+ future |
