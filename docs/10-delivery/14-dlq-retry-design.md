# DLQ / Retry Design

**Status:** Bounded First Slice Exists (Phase 3 DLQ design; app-level DLQ helpers + bounded DLQ metrics worker implemented; G1 solo self-review accepted, G3 local-dev closed after promtool validation; production DLQ worker remains deferred)
**Phase:** Phase 3 bounded — design documented, bounded first slice implemented
**Owner:** Backend Lead / Platform

---

## Purpose

This document specifies the dead-letter queue (DLQ) and retry policy for message-driven workflows in the Intent Rebase Engine. It defines max redeliveries, dead-letter subject naming, manual replay policy, and explicitly gates worker implementation until design is approved.

> **⚠️ Production Readiness Warning**
>
> A **bounded app-level DLQ first slice** is now implemented in `crates/intent-api/src/nats_jetstream.rs` (`DlqHelper` struct and `DlqMetricsWorker`). This is NOT a full production DLQ worker. G1 is closed under solo self-review; G3 is closed for local-dev after promtool validation of alert rules. Full production DLQ worker remains Phase 4+ deferred.
>
> **Bounded local-dev full-consumer gate (commit `0d14c1b`):** `INTENT_API_NATS_FULL_CONSUMER=true` enables app-level DLQ publishing on `Failed`/`Retryable` outcomes **before** ack, plus registration of `SnapshotCreatorConsumer` and `NotifierConsumer`. This is additive, defaults off, and **NOT production-ready**.

---

## Scope

### In Scope (Phase 3 bounded slice)

- DLQ subject naming convention
- Max redeliveries configuration
- Dead-letter routing rules
- Manual replay policy and procedure
- DLQ monitoring and alerting strategy
- **BOUNDED FIRST SLICE**: App-level DLQ helpers implemented (`DlqHelper`)
  - Subject derivation (`derive_dlq_subject()`)
  - Publish to DLQ (`publish_to_dlq()`)
  - Replay primitives (`replay_from_dlq()`, `replay_to_subject()`)
  - DLQ metadata headers
- **BOUNDED FIRST SLICE**: DLQ metrics worker (`DlqMetricsWorker`)
  - Depth/age gauge metric emission
  - Bounded peek-based polling (no message consumption)
  - Behind `INTENT_API_NATS_DLQ_WORKER=true` env gate
  - Requires `INTENT_API_NATS_CONSUMER=true` and `NATS_URL`
- **BOUNDED FIRST SLICE**: DLQ replay worker (`DlqReplayWorker`)
  - Replays messages from DLQ to original subject via `DlqHelper::replay_from_dlq()`
  - ACKs DLQ message only on successful replay; leaves unacked on failure
  - Single subject (`audit.events.v1.DLQ`), bounded `max_replay` per poll
  - Behind `INTENT_API_NATS_DLQ_REPLAY_WORKER=true` env gate
  - Requires `INTENT_API_NATS_CONSUMER=true` and `NATS_URL`
- **BOUNDED LOCAL-DEV (commit `0d14c1b`):** Full-consumer gate (`INTENT_API_NATS_FULL_CONSUMER=true`)
  - App-level DLQ publish on `Failed`/`Retryable` outcomes **before** ack via `NatsPullConsumerAdapter` + `DlqHelper`
  - Registers `SnapshotCreatorConsumer` and `NotifierConsumer` when dependencies available
  - Additive, defaults off, requires `INTENT_API_NATS_CONSUMER=true` + `NATS_URL`
  - Live integration test `live_jetstream_full_consumer_dlq_publish_on_failed` verifies end-to-end DLQ publish (ignored, requires docker-compose NATS)

### Out of Scope (Phase 4+)

- G1: Design approval (closed — solo self-review accepted; original external-SRE dual sign-off criteria not met)
- G2: JetStream consumer `dead_letter` config (CLI/server-side)
- G3: Monitoring/alert rules (closed for local-dev — promtool validated 17 rules; production deployment deferred)
- G4: RB11 runbook update for app-level DLQ
- G5: Integration test coverage (bounded pass — 9 unit + 7 live ignored tests + 1 full-consumer live ignored test + 1 DLQ peek live ignored test)
- **Bounded first slice delivered:** Automatic DLQ replay worker (`DlqReplayWorker`) — production deployment still gated on G1-G5 approvals
- Retry with exponential backoff (future enhancement)
- Per-message-type retry policies (future enhancement)
- DLQ message transformation before replay
- **Production readiness of full-consumer gate:** `INTENT_API_NATS_FULL_CONSUMER` is local-dev only; production deployment requires G1-G5 + external sign-off

---

## Background

The Intent Rebase Engine uses NATS with JetStream for event-driven workflows. When message processing fails repeatedly, messages must be routed to a dead-letter queue to prevent blocking the main queue and to enable manual investigation and replay.

### Current State

- Phase 2b bounded NATS core publisher delivered (`async-nats` 0.47 + core publish)
- JetStream consumers and bounded app-level DLQ helpers delivered
- **BOUNDED FIRST SLICE**: `DlqHelper` struct exists in `nats_jetstream.rs`
  - `derive_dlq_subject()` — safe subject transformation with validation
  - `publish_to_dlq()` — route failed messages to DLQ subject
  - `replay_from_dlq()` / `replay_to_subject()` — replay primitives
  - Metric stubs forward to `lib.rs` record functions
- **BOUNDED FIRST SLICE**: `DlqMetricsWorker` for depth/age metric emission
  - Polls DLQ subjects at configured interval (default: 30s)
  - Emits `intent_api_dlq_messages_current` gauge (depth)
  - Emits `intent_api_dlq_message_age_seconds` gauge (oldest message age)
  - Uses lightweight peek (ack_policy = None) to count without consuming
  - Wired behind `INTENT_API_NATS_DLQ_WORKER=true` env gate
- **BOUNDED FIRST SLICE**: `DlqReplayWorker` for automatic DLQ replay
  - Polls `audit.events.v1.DLQ` at configured interval (default: 60s)
  - Replays up to `max_replay` messages per poll via `DlqHelper::replay_from_dlq()`
  - ACKs DLQ message only on successful replay; leaves unacked on failure for manual investigation
  - Wired behind `INTENT_API_NATS_DLQ_REPLAY_WORKER=true` env gate
- **Bounded local-dev full-consumer gate (commit `0d14c1b`):** `NatsPullConsumerAdapter` accepts `Option<Arc<DlqHelper>>`; `process_one()` publishes to DLQ on `Failed`/`Retryable` before ack; `ConsumerRegistry` supports `with_full_consumer(true)`; `SnapshotCreatorConsumer` + `NotifierConsumer` registered behind gate when dependencies available
- Production DLQ workers NOT YET production-ready (G1 solo self-review, G3 local-dev closed; full worker scope remains Phase 4+ deferred)

### Dependencies

- NATS JetStream must be configured with consumer dead-letter subject
- Maximum deliver attempts must be set on consumers
- DLQ subjects must follow naming convention for discoverability

---

## DLQ Subject Naming Convention

### Format

```
{origin_subject}.DLQ

Or with stream partition:

{origin_subject}.DLQ.{partition}
```

### Examples

| Origin Subject | DLQ Subject |
|----------------|-------------|
| `audit.events.v1.approval.events` | `audit.events.v1.approval.events.DLQ` |
| `audit.events.v1.intent.events` | `audit.events.v1.intent.events.DLQ` |
| `audit.events.v1.forensic.events` | `audit.events.v1.forensic.events.DLQ` |
| `audit.events.v1.policy.events` | `audit.events.v1.policy.events.DLQ` |

> **Note:** The bounded stream is `audit_events` with subject filter `audit.events.v1.>`.
> The `ire.*` naming convention used in older docs is legacy and not current.
> DLQ routing via JetStream consumer `dead_letter` configuration is a future decision —
> async-nats 0.47 does not expose a Rust `dead_letter` field on consumer config.

### Rationale

- `.DLQ` suffix is a standard convention for dead-letter subjects
- Easy to discover via `nats subs list` or JetStream API
- Allows filtering with wildcard `>` or `.DLQ.>`

---

## Max Redeliveries Policy

### Default Configuration

| Setting | Value | Rationale |
|---------|-------|-----------|
| `max_deliver` | 3 | Sufficient for transient failures; not excessive for poison messages |
| `max_deliver_jitter` | 0-30s random | Prevents thundering herd on retry |

### Message-Type-Specific Overrides

| Message Type | Max Deliver | Rationale |
|--------------|-------------|-----------|
| `ApprovalCreated` | 3 | Non-critical; can wait for manual replay |
| `IntentApproved` | 3 | Core workflow; needs prompt retry but not infinite |
| `RebaseApplied` | 3 | Side effects; limited retry budget |
| `ForensicBundleRequested` | 5 | Expensive operation; give more chances before DLQ |
| `CompensationAction` | 5 | Business critical; more retry attempts warranted |

### What Happens at Max Deliveries

1. NATS JetStream consumer marks message as dead-lettered
2. Message is moved to the configured dead-letter subject
3. Consumer ACKs are not sent (message removed from main queue)
4. DLQ consumer can now process the dead-lettered message

---

## Dead-Letter Routing Rules

### JetStream Consumer Configuration

```javascript
// Example: Approval events consumer with DLQ
// Note: dead_letter subject must be configured via CLI/server-side;
// async-nats 0.47 does not expose dead_letter field in Rust consumer config
ConsumerConfig {
  name: "approval-events-processor",
  durable_name: "approval-events-processor",
  stream: "audit_events",
  filter_subject: "audit.events.v1.approval.events",
  max_deliver: 3,
  ack_policy: "explicit",
  ack_wait: 30,  // 30 seconds to process before retry
  // dead_letter: "audit.events.v1.approval.events.DLQ" -- CLI/server-side only
}
```

### Manual Routing Override

For operators who need to manually route a message to DLQ without waiting for max deliveries:

1. Identify the message using `nats msg` or JetStream API
2. Manually publish to DLQ subject:
   ```bash
   nats pub audit.events.v1.approval.events.DLQ --raw "$(nats msg audit.events.v1.approval.events ...)"
   ```
3. ACK the original message to remove from main queue

---

## Manual Replay Policy

### When to Replay from DLQ

| Scenario | Action |
|----------|--------|
| Transient failure (network timeout, DB connection) | Replay immediately or after short delay |
| Bug in consumer code | Fix consumer first, then replay |
| Poison message (malformed payload) | Investigate, do NOT replay until fixed |
| Downstream system outage | Wait for downstream recovery, then replay |
| DLQ accumulation > 100 messages | Alert and prioritize investigation |

### Replay Procedure

#### Step 1: Inspect DLQ

```bash
# List messages in DLQ without consuming
nats consumer next "audit_events" --subject audit.events.v1.approval.events.DLQ --no-ack

# Count messages in DLQ
nats stream ls
nats consumer ls audit_events
```

#### Step 2: Validate Message Content

```bash
# Pull one message and inspect
nats consumer next "audit_events" --subject audit.events.v1.approval.events.DLQ --json
```

Check:
- Message headers (`Nats-Message-Name`, `Nats-Pending`)
- Original subject (`Nats-Orig-Subject`)
- Delivery count (`Nats-Deliver-Count`)
- Payload validity

#### Step 3: Choose Replay Strategy

**Option A: Direct Replay (same subject)**
```bash
# Replay to original subject (bypasses dead-letter config)
nats pub audit.events.v1.approval.events --header "Nats-Replay: true" <payload.json>
```

**Option B: Directed Replay (specific consumer)**
```bash
# Publish directly to the consumer's deliver subject (for pull consumers)
nats pub "audit.events.v1.approval.events.DLQ" <payload.json>
```

**Option C: Batch Replay (multiple messages)**
```bash
# Script to replay all DLQ messages
for msg in $(nats consumer messages audit_events --subject audit.events.v1.approval.events.DLQ --limit 100 2>/dev/null); do
    nats pub audit.events.v1.approval.events "$msg"
done
```

#### Step 4: Monitor During Replay

- Watch consumer lag: `nats consumer ls audit_events`
- Monitor error rates in Grafana
- Check application logs for failures

### Replay Verification Checklist

- [ ] All DLQ messages have been processed (DLQ empty)
- [ ] No new messages appearing in DLQ during replay
- [ ] Application metrics show normal processing rates
- [ ] No error spikes in logs
- [ ] All downstream effects completed successfully

---

## DLQ Monitoring and Alerting

### Metrics to Expose

> **⚠️ Metric Naming Note:** All `intent_api_*` metrics listed below are defined in `crates/intent-api/src/dlq_metrics.rs`. Depth/age gauges and replay failure counter are emitted by `DlqMetricsWorker` when `INTENT_API_NATS_DLQ_WORKER=true`. Full DLQ replay worker remains Phase 4+ deferred.

| Metric | Description | Alert Threshold |
|--------|-------------|-----------------|
| `intent_api_dlq_messages_total` | Total messages ever sent to DLQ | N/A (counter) |
| `intent_api_dlq_messages_current` | Current depth of DLQ | > 10 messages |
| `intent_api_dlq_message_age_seconds` | Age of oldest message in DLQ | > 1 hour |
| `intent_api_dlq_replay_total` | Total replay operations | N/A (counter) |
| `intent_api_dlq_replay_failures_total` | Failed replay attempts | > 0 |

### Prometheus Alert Rules

> **⚠️ Deployment Status:** Alert rules are **deployed to local rules** (`infrastructure/local/prometheus/rules/intent_api_alerts.yml`). `DlqMetricsWorker` emits depth/age gauges and replay failure counter at runtime when enabled. Full DLQ replay worker remains Phase 4+ deferred. **Production alerting requires external SRE routing/signoff and receiver configuration** — do not claim production-ready.

```yaml
groups:
  - name: dlq_alerts
    rules:
      - alert: DLQDepthHigh
        expr: intent_api_dlq_messages_current > 10
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "DLQ depth is high"
          description: "{{ $value }} messages in DLQ"

      - alert: DLQMessageStale
        expr: intent_api_dlq_message_age_seconds > 3600
        for: 5m
        labels:
          severity: critical
        annotations:
          summary: "DLQ message older than 1 hour"
          description: "Oldest message in DLQ is {{ $value }} seconds old"

      - alert: DLQReplayFailures
        expr: rate(intent_api_dlq_replay_failures_total[5m]) > 0
        for: 1m
        labels:
          severity: critical
        annotations:
          summary: "DLQ replay failures detected"
```

---

## Gates for Worker Implementation

The following gates must be PASSED before any DLQ worker code is implemented:

| Gate | Criteria | Owner | Status |
|------|----------|-------|--------|
| G1: Design Approval | This design doc reviewed and approved by Backend Lead + SRE | Backend Lead | ✅ CLOSED (solo self-review) — original external-SRE criteria not met; solo accepted |
| G2: NATS JetStream Config | JetStream streams and consumers configured with DLQ subjects | SRE | ✅ VALIDATED |
| G3: Monitoring Instrumented | DLQ metrics exposed and alerting rules deployed | SRE | ✅ CLOSED (local-dev) — 17 alert rules passed promtool validation; production deployment deferred |
| G4: Runbook Written | DLQ investigation and replay procedure documented in runbooks | SRE | ✅ PASS |
| G5: Test Coverage | Unit tests for retry logic, DLQ routing, and replay | Backend Lead | ✅ PASS (bounded) — 9 unit + 7 live ignored tests |

---

## Implementation Plan (Future Phase 4)

Once all gates are passed:

1. **DLQ Worker Service**
   - Poll DLQ subjects for messages
   - Provide HTTP API for manual replay
   - Emit DLQ metrics

2. **Retry Configuration API**
   - Per-message-type retry policies
   - Dynamic configuration without restart

3. **Automatic Replay (Optional Phase 4 Enhancement)**
   - Automatic retry with exponential backoff
   - Circuit breaker for downstream failures

---

## Related Documents

- [Phase 3 Hardening Plan](./05-phase-3-hardening.md)
- [S3 Snapshot Blob Specification](../14-governance/05-s3-snapshot-blob-spec.md)
- [Trace Propagation Blocker Matrix](./12-trace-propagation-blocker-matrix.md)
- [Runbooks](../09-operations/05-runbooks.md)
- [SLO and Alerting](../09-operations/04-sre-and-slos.md)
