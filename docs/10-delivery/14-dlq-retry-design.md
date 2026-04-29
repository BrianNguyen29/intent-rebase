# DLQ / Retry Design

**Status:** Proposed (Phase 3 — Not Implemented)
**Phase:** Phase 3 bounded slice (design only; worker implementation gated)
**Owner:** Backend Lead / Platform

---

## Purpose

This document specifies the dead-letter queue (DLQ) and retry policy for message-driven workflows in the Intent Rebase Engine. It defines max redeliveries, dead-letter subject naming, manual replay policy, and explicitly gates worker implementation until design is approved.

> **⚠️ Production Readiness Warning**
>
> This specification describes **design for future implementation**. No DLQ worker code exists yet. Do not claim production-ready retry/DLQ handling until this design is approved and implemented.

---

## Scope

### In Scope (Phase 3 bounded slice)

- DLQ subject naming convention
- Max redeliveries configuration
- Dead-letter routing rules
- Manual replay policy and procedure
- DLQ monitoring and alerting strategy
- **Design doc only** — no worker implementation

### Out of Scope (Phase 4+)

- Automatic DLQ replay worker (gated on this design approval)
- Retry with exponential backoff (future enhancement)
- Per-message-type retry policies (future enhancement)
- DLQ message transformation before replay

---

## Background

The Intent Rebase Engine uses NATS with JetStream for event-driven workflows. When message processing fails repeatedly, messages must be routed to a dead-letter queue to prevent blocking the main queue and to enable manual investigation and replay.

### Current State

- Phase 2b bounded NATS core publisher delivered (`async-nats` 0.47 + core publish)
- JetStream consumers and DLQ routing are Phase 3 scope
- No retry worker exists

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

| Metric | Description | Alert Threshold |
|--------|-------------|-----------------|
| `dlq_messages_total` | Total messages ever sent to DLQ | N/A (counter) |
| `dlq_messages_current` | Current depth of DLQ | > 10 messages |
| `dlq_message_age_seconds` | Age of oldest message in DLQ | > 1 hour |
| `dlq_replay_total` | Total replay operations | N/A (counter) |
| `dlq_replay_failures_total` | Failed replay attempts | > 0 |

### Prometheus Alert Rules

```yaml
groups:
  - name: dlq_alerts
    rules:
      - alert: DLQDepthHigh
        expr: dlq_messages_current > 10
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "DLQ depth is high"
          description: "{{ $value }} messages in DLQ"

      - alert: DLQMessageStale
        expr: dlq_message_age_seconds > 3600
        for: 5m
        labels:
          severity: critical
        annotations:
          summary: "DLQ message older than 1 hour"
          description: "Oldest message in DLQ is {{ $value }} seconds old"

      - alert: DLQReplayFailures
        expr: rate(dlq_replay_failures_total[5m]) > 0
        for: 1m
        labels:
          severity: critical
        annotations:
          summary: "DLQ replay failures detected"
```

---

## Gates for Worker Implementation

The following gates must be PASSED before any DLQ worker code is implemented:

| Gate | Criteria | Owner |
|------|----------|-------|
| G1: Design Approval | This design doc reviewed and approved by Backend Lead + SRE | Backend Lead |
| G2: NATS JetStream Config | JetStream streams and consumers configured with DLQ subjects | SRE |
| G3: Monitoring Instrumented | DLQ metrics exposed and alerting rules deployed | SRE |
| G4: Runbook Written | DLQ investigation and replay procedure documented in runbooks | SRE |
| G5: Test Coverage | Unit tests for retry logic, DLQ routing, and replay | Backend Lead |

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
