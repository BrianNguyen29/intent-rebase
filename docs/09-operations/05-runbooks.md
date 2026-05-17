# Runbooks

## RB1. Diff service degraded
- check model/rules dependencies
- enable rules-only fallback
- route low-confidence to manual review
- notify tenants if SLA breached

## RB2. Queue lag high
- inspect top tenants/workflows
- enable per-tenant throttling
- increase worker pool if safe
- protect apply path first

## RB3. Runtime adapter failing apply
- freeze affected adapter
- disable auto-apply for impacted tenants
- switch to preview-only mode
- open incident

## RB4. Audit sink unavailable
- use local durable buffer
- protect append-only primary store
- alert security/platform

## RB5. Compensation failures
- classify side effect severity
- retry if idempotent
- escalate operator if irreversible/partial

---

## RB6. Rebase stuck
A rebase operation is stuck (no progress, not completing, not erroring) and the rebase plan is not advancing.

**Symptoms:**
- Rebase plan status remains `in_progress` or `pending` beyond expected duration
- No new artifacts being produced
- No error responses but plan not advancing
- Worker pool appears idle despite pending rebase work

**Diagnosis:**
1. Check rebase-engine logs for stack traces or panics: `kubectl logs -l app=rebase-engine | grep -i "panicked\|thread.*panicked"`
2. Inspect the rebase plan state in the database:
   ```sql
   SELECT id, intent_id, status, decision_class, created_at, updated_at
   FROM rebase_plans
   WHERE status IN ('in_progress', 'pending')
   ORDER BY updated_at ASC
   LIMIT 20;
   ```
3. Check if the runtime adapter is healthy and responsive
4. Verify dependency services (graph-service, intent-service) are reachable

**Mitigation:**
1. **If engine panic detected:** Restart rebase-engine pods; existing plans resume from checkpoint if available
2. **If dependency unavailable:** Route to preview-only mode per RB1 (diff service degraded); rebase-stuck is a subset of diff service degraded patterns
3. **If plan legitimately blocked on upstream:** Set rebase plan status to `blocked_manual_review` and notify tenant
4. **If indefinite hang with no clear cause:** Capture diagnostic bundle (intent versions, plan state, recent logs) and escalate to backend lead

**Recovery:**
- Plans with checkpoints can be resumed from the last known good state
- Plans without checkpoints may need to be restarted from the diff baseline
- Document root cause in incident tracker

**Prevention:**
- Ensure runtime adapter health checks are configured (see RB3)
- Monitor rebase plan age: alert if any plan exceeds 30 minutes in `in_progress` status
- Configure request timeouts on runtime adapter calls to prevent indefinite hangs

---

## Batch 2 Runbooks (Phase 3 Batch 2 Slice 3)

### RB7. Approval backlog / stale approvals

**Symptoms:**
- `GET /approval-requests/pending` returns growing queue
- p95 approval wait time exceeds 30 minute threshold

**Diagnosis:**
1. `intent_api_approval_wait_duration_seconds` is **not instrumented** — panel removed from dashboard
2. List pending approval requests: `GET /approval-requests/pending?tenant_id=<id>`
3. Check if runtime adapter is functioning (blocked applies require approvals)

**Mitigation:**
1. Notify approvers via configured notification channel
2. If SLA is breached: open incident and notify tenant
3. If approval workflow is broken: check Temporal workflow state

**Recovery:**
- Pending approvals can be manually approved via `POST /approval-requests/{id}/approve`
- Or expired via `POST /approval-requests/{id}/expire`

---

### RB8. Artifact quarantine failures

**Symptoms:**
- Artifact ingest returns error but artifact is recorded
- DLQ candidate count elevated in Grafana

**Diagnosis:**
1. `intent_api_side_effect_capture_errors_total` is **not instrumented** — panel removed from dashboard
2. List DLQ candidates: `GET /compensation-actions/dlq`
3. Check artifact storage connectivity (MinIO/S3)

**Mitigation:**
1. If artifact storage is unavailable: artifacts are buffered locally
2. Once storage recovers: re-trigger capture via retry endpoint
3. If quarantine fails permanently: manual cleanup required

**Recovery:**
- Artifacts are not lost — they remain in local buffer until quarantine succeeds
- `intent_api_side_effect_captured_total` is **not instrumented** — use application logs to confirm recovery

---

### RB9. Compensation timeout

**Symptoms:**
- Compensation execution never completes
- `GET /compensation-actions/{id}` shows status = "executed" but side effects not reversed

**Diagnosis:**
1. `intent_api_compensation_execution_total` is **not instrumented** — panel removed from dashboard; use application logs instead
2. List compensation actions: `GET /compensation-actions/batch-candidates`
3. Check compensation service logs for executor errors

**Mitigation:**
1. If executor failed: action status becomes "failed" after max retries
2. Check DLQ: `GET /compensation-actions/dlq`
3. If terminal failure: waive or manually resolve compensation

**Recovery:**
- Failed actions can be re-approved if retry budget remains: `POST /compensation-actions/{id}/reapprove`
- If budget exhausted: manual intervention required

---

### RB10. Error budget burn rate alert

**Symptoms:**
- Alert fires: one of `PreviewPathBurnRate1h`, `PreviewPathBurnRate6h`, `PreviewPathBurnRate3d` (preview path) or `ApplyPathBurnRate1h`, `ApplyPathBurnRate6h`, `ApplyPathBurnRate3d` (apply path)
- Error budget panel shows rapid consumption

**Diagnosis:**
1. Check which SLO is being breached (preview availability vs apply availability) and which window is firing (1h = fast, 6h = sustained, 3d = chronic)
2. Identify error patterns in logs: `status!="success"` on relevant counter

**Mitigation:**
1. Open incident if multiple SLOs are burning
2. Prioritize fixing apply path errors (lower error budget = more urgent)
3. If preview path degraded: switch tenants to preview-only mode

**Recovery:**
- Error budgets recover over time if error rate returns to normal
- If budget is exhausted: SLO is breached until error rate improves
- Post-incident review should identify root cause

---

## RB11. DLQ Messages Found

> **Status:** Bounded observability slice — DLQ alert rules (`DLQDepthHigh`, `DLQMessageStale`, `DLQReplayFailures`) are defined in `infrastructure/local/prometheus/rules/intent_api_alerts.yml` as **local dev scaffolding only**. Metrics are emitted by `DlqMetricsWorker` when `INTENT_API_NATS_DLQ_WORKER=true` (depth/age gauges and replay failure counter). Full DLQ replay worker remains Phase 4+ deferred. **Production alerting requires external SRE routing/signoff and receiver configuration** — do not claim production-ready.

**Symptoms:**
- `nats stream ls` or `nats consumer ls` shows messages in DLQ subjects
- Application logs show repeated delivery failures for same `Nats-Message-Name`

> **Note:** DLQ routing via JetStream consumer `dead_letter` configuration is a **future
> decision**. The current `NatsPullConsumerAdapter` aligns with the G2 retry config
> (`max_deliver=3`, `ack_wait=30s`) without automatic DLQ routing. If DLQ routing is desired, it requires either:
> - Server-side/CLI consumer configuration with `dead_letter` subject
> - Application-level manual routing to a DLQ subject
>
> async-nats 0.47 does not expose a Rust `dead_letter` field on consumer config.
> See `crates/intent-api/src/nats_jetstream.rs` for current bounded behavior.

**Diagnosis:**
1. Inspect DLQ subject count:
   ```bash
   docker compose -f infrastructure/local/docker-compose.yml exec nats nats stream ls
   docker compose -f infrastructure/local/docker-compose.yml exec nats nats consumer ls audit_events
   ```
2. Pull sample DLQ message to inspect:
   ```bash
   docker compose -f infrastructure/local/docker-compose.yml exec nats nats consumer next "audit_events" --subject audit.events.v1.approval.events.DLQ --json
   ```
   > **Note:** DLQ subject naming follows `audit.events.v1.{event_type}.DLQ` convention
   > (not `ire.*` — that naming is legacy and not current).
3. Check message headers for:
   - `Nats-Deliver-Count` (delivery attempt count)
   - `Nats-Message-Name` (correlation ID for logs)
   - `Nats-Orig-Subject` (original subject before DLQ)
4. Check application logs for the `Nats-Message-Name` correlation

**Mitigation:**
1. If transient failure (network timeout, DB connection):
   - Replay immediately or after short delay
   ```bash
   # Direct replay to original subject
   docker compose -f infrastructure/local/docker-compose.yml exec nats nats pub audit.events.v1.approval.events --header "Nats-Replay: true" <payload.json>
   ```
   > **Note:** The stream is `audit_events` with subject filter `audit.events.v1.>`.
   > Legacy commands using `ire.*` subjects or `INTENT_EVENTS` stream are not current.
2. If bug in consumer code:
   - Fix consumer code FIRST
   - Then replay DLQ messages
3. If poison message (malformed payload):
   - **Do NOT replay** until payload is fixed
   - Investigate root cause of malformation
4. If downstream system outage:
   - Wait for downstream recovery
   - Then replay DLQ messages

**Recovery:**
1. Monitor DLQ depth after replay:
   ```bash
   docker compose -f infrastructure/local/docker-compose.yml exec nats nats stream ls
   # DLQ depth should return to 0
   ```
2. Watch consumer lag:
   ```bash
   docker compose -f infrastructure/local/docker-compose.yml exec nats nats consumer ls audit_events
   ```
3. Verify no new DLQ messages appearing during replay
4. Check application logs for `Nats-Message-Name` correlation

**Prevention:**
- `intent_api_dlq_messages_current`, `intent_api_dlq_message_age_seconds`, and `intent_api_dlq_replay_failures_total` are instrumented by `DlqMetricsWorker` (behind `INTENT_API_NATS_DLQ_WORKER=true` gate) — local alert rules exist; production alerting still requires SRE sign-off
- Review DLQ messages weekly if volume is non-zero
- Set `max_deliver` appropriately per message type (see `14-dlq-retry-design.md`)

---

---

## RB12. Propagation Signal Creation Failures

> **Status:** Bounded observability slice — metrics instrumented, no production alerting claim. Webhook delivery and event streaming remain Phase 4+ deferred.

**Symptoms:**
- `intent_api_propagation_signals_failed_total` counter is increasing
- Warning logs: `Failed to update propagation signal for system {id}` or `Failed to list propagation records for signal creation`
- Downstream systems not receiving updated propagation status after rebase apply

**Diagnosis:**
1. Check propagation signal metrics on `/metrics`:
   ```
   intent_api_propagation_signals_attempted_total
   intent_api_propagation_signals_succeeded_total
   intent_api_propagation_signals_failed_total
   intent_api_propagation_signals_no_downstream_total
   ```
2. If `failed_total` is increasing while `attempted_total` is also increasing:
   - Check application logs for `Failed to update propagation signal` warnings
   - Verify `propagation_records` table is accessible (DB connection healthy)
   - Check for RLS context issues: `SET LOCAL app.current_tenant_id` must be set correctly in transactions
3. If `no_downstream_total` is increasing:
   - This is expected behavior when no downstream systems are registered for an intent
   - Use `POST /intents/{intent_id}/propagation-signals` to register downstream systems manually

**Mitigation:**
1. If transient DB error (connection timeout, pool exhaustion):
   - Monitor `intent_api_propagation_signals_succeeded_total` recovery
   - Signal creation is best-effort — apply response is NOT affected
2. If persistent `propagation_records` table access failure:
   - Check migration 017 was applied: `SELECT COUNT(*) FROM propagation_records`
   - Verify RLS policies are active on `propagation_records`
   - Check table owner is not bypassing RLS: `SELECT relforcerowsecurity FROM pg_class WHERE relname = 'propagation_records'`
3. If downstream system should be registered but is not:
   - Manually register via signal ingestion endpoint (see **Manual Re-Signal Workflow** below)

**Manual Re-Signal Workflow:**

Use the bounded signal ingestion endpoint to register or re-register a downstream system:

```bash
POST /intents/{intent_id}/propagation-signals
Content-Type: application/json

{
  "tenant_id": "<tenant-uuid>",
  "downstream_system_id": "workflow-runner-a",
  "last_seen_version": 3
}
```

This creates a new `pending` propagation record. Subsequent rebase apply operations will automatically update this record to `pending` with the new version.

**Recovery:**
1. After fixing root cause, verify `succeeded_total` increases on next rebase apply
2. Query propagation status to confirm downstream systems are visible:
   ```bash
   GET /intents/{intent_id}/propagation-status?tenant_id=<tenant-uuid>
   ```
3. If records are stale (wrong version), manually update via the ingestion endpoint

**Prevention:**
- Register downstream systems proactively via `POST /intents/{intent_id}/propagation-signals`
- Monitor `intent_api_propagation_signals_failed_total / attempted_total` ratio; alert if > 10% sustained
- `intent_api_propagation_signals_no_downstream_total` is informational only — no action required
- Local Prometheus rule `PropagationSignalFailureRate` is defined in `infrastructure/local/prometheus/rules/intent_api_alerts.yml` (local dev scaffolding only — production alerting requires SRE sign-off and receiver configuration)

---

## RB13. Webhook Delivery Failures

> **Status:** Bounded observability slice — metrics instrumented, no production alerting claim. Webhook delivery remains Phase 4+ deferred; no production delivery guarantees, outbox, event streaming, HMAC, key rotation, subscription CRUD, retry/DLQ, or rollback automation. See P2-6e retry / dead-letter semantics design and P2-6f rollback plan design in [Production Readiness Backlog](../10-delivery/17-production-readiness-backlog.md).

**Symptoms:**
- `intent_api_webhook_deliveries_failed_total` counter is increasing
- `intent_api_webhook_deliveries_retry_exhausted_total` counter is increasing
- Warning logs: `Failed to record delivery outcome for record {id}` or `Failed to record delivery attempt for record {id}`
- Downstream systems not receiving webhook callbacks after rebase apply

**Diagnosis:**
1. Check webhook delivery metrics on `/metrics`:
   ```
   intent_api_webhook_deliveries_attempted_total
   intent_api_webhook_deliveries_succeeded_total
   intent_api_webhook_deliveries_failed_total
   intent_api_webhook_deliveries_retry_exhausted_total
   ```
2. If `failed_total` is increasing while `attempted_total` is also increasing:
   - Check application logs for `Unexpected delivery error after retries` or `Failed to record delivery outcome`
   - Verify downstream webhook URLs are reachable from the intent-api network
   - Check for HTTP 4xx responses (non-retryable) vs HTTP 5xx / network errors (retryable)
3. If `retry_exhausted_total` is increasing:
   - Downstream system may be consistently returning 5xx or unreachable
   - Check if `WEBHOOK_MAX_ATTEMPTS` (3) and backoff delays are appropriate for the downstream SLA
4. If delivery attempts are not occurring at all:
   - Verify `INTENT_API_WEBHOOK_DELIVERY` env var is set to `true` (default is disabled)
   - Verify `webhook_subscriptions` table has rows for the affected intent
   - Check propagation records exist for the downstream system

**Mitigation:**
1. If transient downstream error (5xx, timeout):
    - Retry-exhausted records remain in `Failed` status; no automatic retry queue exists in this slice
    - Manual re-trigger requires updating the propagation record status to `Pending` and running a new rebase apply (if webhook delivery is enabled)
2. If non-retryable error (4xx except 429):
    - Fix the downstream webhook URL or payload contract
    - Update the subscription URL in `webhook_subscriptions` if incorrect (manual DB update — no CRUD API in this slice)
3. If webhook delivery is disabled (default):
    - Set `INTENT_API_WEBHOOK_DELIVERY=true` in the environment
    - Restart intent-api pods to pick up the change
    - This is an opt-in gate — do not enable in production without SRE review
4. If RLS context issues:
    - Verify `SET LOCAL app.current_tenant_id` is set correctly in transactions
    - Check `webhook_subscriptions` rows are visible under the tenant's RLS context

**Local-Dev DLQ List/Replay (Slice 5b — bounded, non-production):**
> These endpoints are local-dev only and NOT wired for production. No production retention, operator workflow, or replay UI exists.

- List failed outbox records: `GET /webhooks/outbox/dlq?tenant_id=<uuid>[&limit=<n>]`
  - Returns `WebhookOutboxStatus::Failed` records ordered by `updated_at` desc
  - Empty list when no failures or when `webhook_outbox_repo` is not configured
- Replay a failed record: `POST /webhooks/outbox/dlq/:id/replay?tenant_id=<uuid>[&replayed_by=<actor>]`
  - Transitions record from `Failed` to `Pending`, resets `attempt_count=0`, clears `last_error`/`locked_at`/`locked_by`
  - Idempotency-bounded: only `Failed` records can be replayed; second replay returns an error because status is no longer `Failed`
  - After replay, the worker will pick up the record on its next pass if `INTENT_API_WEBHOOK_OUTBOX_WORKER=true`
  - Phase 1.2 replay metadata: increments `replay_count`, sets `replayed_at=now`, sets `replayed_by` from query param if provided
- Replay audit query (Phase 1.3 — bounded local-dev):
  - List replayed records: `GET /webhooks/outbox/dlq/replayed?tenant_id=<uuid>[&limit=<n>][&since=<rfc3339>]`
  - Returns records with `replay_count > 0` and `replayed_at` present, ordered by `replayed_at` desc
  - Optional `since` filter returns only records replayed at or after the given RFC 3339 timestamp
  - Empty list when no replayed records or when `webhook_outbox_repo` is not configured
  - **No production audit trail claim:** this is a convenience query for local development only; it does not replace a production-grade audit log, SIEM integration, or compliance evidence
- Retention query (Phase 1.1 — query-only, no purge/enforcement):
  - List failed records older than a cutoff via `WebhookOutboxRepository::list_failed_older_than(tenant_id, before, limit)`
  - Query-only local-dev helper; no delete endpoint, no background job, no S3/Object Lock
  - `WEBHOOK_OUTBOX_RETENTION_DAYS` env var is available for documentation only (no enforcement logic)
- Caveats:
  - No separate DLQ table — uses existing `webhook_outbox` `status='failed'` rows
  - No operator UI or batch replay API
  - Production audit trail, operator workflow, and replay UI remain deferred

**Rollback Boundaries:**
- Webhook delivery is best-effort and does NOT affect rebase apply outcomes
- Disabling `INTENT_API_WEBHOOK_DELIVERY` immediately stops all delivery attempts without affecting apply path
- Propagation records remain in their current state (Acknowledged/Failed/Pending) and can be inspected directly

**Recovery:**
1. After fixing root cause, verify `succeeded_total` increases on the next rebase apply with delivery enabled
2. Query propagation status to confirm downstream systems are visible:
   ```bash
   GET /intents/{intent_id}/propagation-status?tenant_id=<tenant-uuid>
   ```
3. For failed records, manually update `propagation_records` status to `Pending` to allow re-signal on next apply

**Prevention:**
- Register downstream systems proactively via `POST /intents/{intent_id}/propagation-signals` (creates both propagation record and webhook subscription)
- Monitor `intent_api_webhook_deliveries_failed_total / attempted_total` ratio; alert if > 10% sustained
- `intent_api_webhook_deliveries_retry_exhausted_total` is informational — indicates persistent downstream issues
- Local Prometheus rule `WebhookDeliveryFailureRate` is defined in `infrastructure/local/prometheus/rules/intent_api_alerts.yml` (local dev scaffolding only — production alerting requires SRE sign-off and receiver configuration)
- Outbox DLQ and automatic retry queue are design-only (P2-6e). No queue, table, or worker exists.
- Rollback plan (env-gate disable, worker drain, subscription deregister) is design-only (P2-6f). No automation or scripts exist.

---

## RB14. Webhook DLQ / Replay Operator Workflow

> **Status:** Phase 2.1 documentation-only runbook preparation. **NOT externally reviewed, NOT staging/production validated, NOT production readiness evidence by itself.** This runbook describes the intended operator workflow for webhook outbox DLQ and replay operations once production operator tooling, replay UI, and external sign-off are in place. All referenced APIs are local-dev only (see RB13). Production operator workflow validation, replay UI, retention enforcement, and WEB-EXT blockers remain deferred.

**Symptoms:**
- Downstream systems report missing webhook callbacks
- `intent_api_webhook_deliveries_retry_exhausted_total` is increasing
- Failed outbox records are not being retried automatically
- Worker metrics show elevated claim/lock ages

**Pre-Flight Checks:**
1. Verify `INTENT_API_WEBHOOK_DELIVERY` is enabled (default is `false`)
2. Verify `INTENT_API_WEBHOOK_OUTBOX_WORKER` is enabled (default is `false`)
3. Verify downstream webhook URLs are reachable from the intent-api network
4. Confirm no deployment or configuration change is in progress

---

### Step 1 — Triage Failed Deliveries

**List failed outbox records:**
```bash
GET /webhooks/outbox/dlq?tenant_id=<uuid>&limit=100
```

**Classify each failure:**

| Failure Pattern | Likely Cause | Action |
|-----------------|--------------|--------|
| HTTP 4xx (except 429) | Invalid URL, auth mismatch, bad payload | Fix downstream contract or subscription URL; do NOT replay until fixed |
| HTTP 429 | Rate limit | Wait for rate-limit window; replay after delay |
| HTTP 5xx / timeout | Transient downstream error | Safe to replay (see Step 3) |
| Network unreachable | DNS, firewall, downstream outage | Verify downstream health before replay |
| `locked_at` very old | Stale worker claim | See Step 2 — Stale Claim Recovery |

**Retention query (query-only, no purge):**
```bash
# List failed records older than 7 days (example)
# There is no dedicated HTTP endpoint for this; use repository helper directly
# or query the database:
SELECT id, tenant_id, intent_id, subscription_id, attempt_count, last_error, updated_at
FROM webhook_outbox
WHERE status = 'failed' AND updated_at < NOW() - INTERVAL '7 days'
ORDER BY updated_at DESC
LIMIT 100;
```

---

### Step 2 — Stale Claim Recovery

A record in `Claimed` status with an old `locked_at` indicates a worker crashed or was terminated before releasing the claim.

**Diagnosis:**
```sql
SELECT id, tenant_id, locked_at, locked_by, attempt_count
FROM webhook_outbox
WHERE status = 'claimed' AND locked_at < NOW() - INTERVAL '5 minutes'
ORDER BY locked_at ASC
LIMIT 50;
```

**Recovery (manual DB update — no automation in this slice):**
```sql
UPDATE webhook_outbox
SET status = 'pending',
    locked_at = NULL,
    locked_by = NULL,
    lock_version = lock_version + 1,
    updated_at = NOW()
WHERE id = '<record-uuid>' AND tenant_id = '<tenant-uuid>' AND status = 'claimed';
```

> **Caveat:** There is no automated stale-claim detector or self-healing worker. Manual intervention or a future background reconciliation job is required.

---

### Step 3 — Replay Decision Tree

```text
Is the failure transient (5xx, timeout, 429 after window)?
  ├── YES → Is this a single record or multiple records?
  │           ├── Single → Use single replay API (Step 3a)
  │           └── Multiple → Use bounded bulk replay API (Step 3b)
  │                         → Hard cap of 100 records per call; no UI
  └── NO (4xx, auth error, contract mismatch)
      → Fix root cause FIRST
      → Then replay
```

**3a — Single Replay**
```bash
POST /webhooks/outbox/dlq/<record-id>/replay?tenant_id=<uuid>&replayed_by=operator-<id>
```

- Record transitions from `Failed` → `Pending`
- `attempt_count` resets to 0
- `replay_count` increments, `replayed_at` set to now, `replayed_by` set to operator ID
- Worker will pick up the record on next poll if `INTENT_API_WEBHOOK_OUTBOX_WORKER=true`

**3b — Bulk Replay (Phase 2.2 — bounded local-dev)**
```bash
POST /webhooks/outbox/dlq/bulk-replay
Content-Type: application/json

{
  "tenant_id": "<uuid>",
  "max_records": 50,
  "replayed_by": "operator-<id>"
}
```

- Replays up to `max_records` failed records (hard cap of 100 enforced server-side regardless of request value)
- Uses existing `replay_failed` per-record guard; only `Failed` records are replayed
- Returns `replayed`, `skipped`, and `errors` counts plus the list of successfully replayed records
- Records no longer in `Failed` status when replay is attempted are counted as `skipped` (race condition)
- **Not a production batch operator tool:** no UI, no staging/production validation, no automatic scheduling

**3c — Verify Replay**
```bash
GET /webhooks/outbox/dlq/replayed?tenant_id=<uuid>&limit=10&since=<rfc3339>
```

- Confirms the replay was recorded with metadata
- Use `since` to narrow to recent replays
- **No production audit trail claim:** this query is a convenience helper, not a compliance-grade audit log

**3d — Replay Safety Rules**
- **Do NOT replay** if the downstream URL or payload contract is known to be broken
- **Do NOT replay** more than once without verifying downstream recovery
- **Do NOT replay** poison messages (records that consistently fail with non-retryable errors)
- **Do NOT use bulk replay** as an automatic retry mechanism — it is a manual operator action

---

### Step 4 — Rollback via Environment Gates

If webhook delivery is causing systemic issues (cascading failures, downstream overload, data corruption risk):

**Immediate stop (no code deploy required):**
```bash
# Set env var and restart intent-api pods
INTENT_API_WEBHOOK_DELIVERY=false
INTENT_API_WEBHOOK_OUTBOX_WORKER=false
```

**Effect:**
- New delivery attempts stop immediately
- Existing worker claims are not automatically released (see Step 2 for manual cleanup)
- Rebase apply path is NOT affected — webhook delivery is best-effort only

**Rollback boundaries:**
- Disabling delivery does not modify any outbox record state
- Records remain in their current status (Pending/Claimed/Failed/Delivered)
- Re-enabling delivery resumes normal worker polling from current state
- No data loss on the apply path

---

### Step 5 — Severity / Escalation

| Severity | Criteria | Operator Action | Escalation |
|----------|----------|-----------------|------------|
| **Low** | Fewer than 5 failed records, transient errors | Replay after verifying downstream health | None — document in incident tracker |
| **Medium** | 5–50 failed records, sustained 5xx from one downstream | Stop delivery to that downstream; replay after fix | Notify backend lead if not resolved within 1 hour |
| **High** | More than 50 failed records, multiple downstreams affected, or data integrity risk | Disable `INTENT_API_WEBHOOK_DELIVERY` globally; open incident | Escalate to backend lead immediately |
| **Critical** | Webhook delivery causing apply path degradation, or suspected security incident | Disable all webhook and worker gates; preserve logs and outbox state | Open incident, page on-call, involve security if tampering suspected |

---

### Recovery

1. After root cause is fixed, re-enable delivery/worker gates
2. Replay failed records individually (no bulk replay in this slice)
3. Monitor `intent_api_webhook_deliveries_succeeded_total` for recovery
4. Use replay audit query to verify operator actions were recorded:
   ```bash
   GET /webhooks/outbox/dlq/replayed?tenant_id=<uuid>&limit=50&since=<start-of-incident-rfc3339>
   ```
5. Document all replays and rollbacks in the incident tracker

---

### Prevention

- Monitor `intent_api_webhook_deliveries_failed_total / attempted_total` ratio
- Alert if `retry_exhausted_total` increases faster than `succeeded_total`
- Review replay audit query weekly during active webhook operations
- Maintain accurate downstream system health checks
- Keep subscription URLs and payload contracts under version control

---

### Caveats and Deferred Items

- **No external review:** this runbook has not been reviewed by an external SRE or operations team
- **No staging/production validation:** the workflow described has not been executed in a staging or production environment
- **No replay UI:** all replay actions require direct API calls or database access
- **Bulk replay is bounded local-dev only:** `POST /webhooks/outbox/dlq/bulk-replay` has a hard cap of 100 records per call, no UI, no staging/production validation, and no automatic scheduling. It is a convenience API, not a production batch operator tool
- **No automated stale-claim recovery:** manual SQL or future background job required
- **No retention enforcement:** `WEBHOOK_OUTBOX_RETENTION_DAYS` is documented but not wired to any purge logic
- **Production readiness blockers remain open:** WEB-EXT-1 (secret manager), WEB-EXT-2 (staging/prod evidence), WEB-EXT-3 (external review/pen-test) are all still blocked
- **This runbook is a planning artifact:** it does not constitute production readiness evidence by itself

---

## On-Call Quick Reference

| Alert | Severity | Immediate Action |
|-------|----------|------------------|
| IntentVersionCreationLowSuccessRate | Warning | Check service health, rollback recent changes |
| RebasePreviewLowAvailability | Warning | Check graph-service connectivity |
| RebaseApplyLowAvailability | Critical | Check runtime adapter, open incident |
| DiffComputeHighLatency | Warning | Check rebase-engine CPU/memory |
| RebasePreviewHighLatency | Warning | Check graph size, consider preview-only mode |
| RebaseApplyHighLatency | Warning | Check runtime adapter health |
| PreviewPathBurnRate1h/6h/3d | Warning | Monitor burn rate windows; prepare incident if 1h persists |
| ApplyPathBurnRate1h/6h/3d | Critical | Open incident, prioritize fix — check which window is firing |
| PropagationSignalFailureRate | Warning | Check DB connectivity and RLS policy health; see RB12 |
| WebhookDeliveryFailureRate | Warning | Check downstream URL health and webhook delivery metrics; see RB13 |
| WebhookDlqReplayNeeded | Warning | Triage failed deliveries, check stale claims, decide single replay; see RB14 |
| DLQDepthHigh | Warning | Check DLQ subject count and replay messages; see RB11 |
| DLQMessageStale | Warning | Investigate oldest DLQ message; see RB11 |
| DLQReplayFailures | Warning | Check replay logs and consumer health; see RB11 |
| LocalAlertReceiver | Info | Standalone: `python3 infrastructure/local/alertmanager/webhook_receiver.py` → http://localhost:9094/webhook; Docker Compose: `docker compose -f infrastructure/local/docker-compose.yml --profile observability up -d` → Alertmanager routes to `alert-receiver:9094` internally; local/manual-only — not a production receiver |

> **Removed alerts (metrics not instrumented):** `CompensationDLQCandidatesElevated` — panel and rule cleaned up as part of stale observability cleanup. DLQ alerts are now present as local dev scaffolding.
> **Propagation alerts:** `PropagationSignalFailureRate` is documented in RB12 and defined in `infrastructure/local/prometheus/rules/intent_api_alerts.yml` as local dev scaffolding only; production alerting still requires SRE sign-off and receiver configuration.

---

## Local Observability Smoke Checklist

> **Local dev only.** Use this checklist to verify the Alertmanager → alert-receiver delivery path on a developer workstation. Not a production readiness check.

- [ ] **Validate compose config**
  ```bash
  docker compose -f infrastructure/local/docker-compose.yml --profile observability config
  ```
- [ ] **Start alert-receiver and alertmanager** (preserves existing Postgres/NATS/MinIO)
  ```bash
  docker compose -f infrastructure/local/docker-compose.yml --profile observability up -d alert-receiver alertmanager
  ```
- [ ] **Run smoke helper**
  ```bash
  python3 infrastructure/local/alertmanager/smoke_test_alert_receiver.py
  ```
- [ ] **Inspect alert-receiver logs** for `TestAlert` payload
  ```bash
  docker compose -f infrastructure/local/docker-compose.yml --profile observability logs alert-receiver
  ```
- [ ] **Clean up** while preserving pre-existing core services
  ```bash
  docker compose -f infrastructure/local/docker-compose.yml --profile observability stop alert-receiver alertmanager
  docker compose -f infrastructure/local/docker-compose.yml --profile observability rm -f alert-receiver alertmanager
  ```
  Or stop the entire observability profile:
  ```bash
  docker compose -f infrastructure/local/docker-compose.yml --profile observability down
  ```
