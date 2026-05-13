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

> **⚠️ Deployment Status:** DLQ alert rules (`DLQDepthHigh`, `DLQMessageStale`, `DLQReplayFailures`) were previously defined in `infrastructure/local/prometheus/rules/intent_api_alerts.yml` but reference metrics that are **not instrumented** (`intent_api_dlq_messages_current`, `intent_api_dlq_message_age_seconds`, `intent_api_dlq_replay_failures_total`). These alerts have been **removed from local rules** as part of stale observability cleanup. **Production alerting requires external SRE routing/signoff** — do not claim production-ready.

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
- `intent_api_dlq_messages_current`, `intent_api_dlq_message_age_seconds`, and `intent_api_dlq_replay_failures_total` are **not instrumented** — alerts removed from local rules
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
| LocalAlertReceiver | Info | Standalone: `python3 infrastructure/local/alertmanager/webhook_receiver.py` → http://localhost:9094/webhook; Docker Compose: `docker compose -f infrastructure/local/docker-compose.yml --profile observability up -d` → Alertmanager routes to `alert-receiver:9094` internally; local/manual-only — not a production receiver |

> **Removed alerts (metrics not instrumented):** `CompensationDLQCandidatesElevated`, `DLQDepthHigh`, `DLQMessageStale` — panels and rules cleaned up as part of stale observability cleanup.
> **Propagation alerts:** `PropagationSignalFailureRate` is documented in RB12 and defined in `infrastructure/local/prometheus/rules/intent_api_alerts.yml` as local dev scaffolding only; production alerting still requires SRE sign-off and receiver configuration.
