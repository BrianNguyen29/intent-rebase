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
1. Check `intent_api_approval_wait_duration_seconds` panel if instrumented
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
1. Check `intent_api_side_effect_capture_errors_total` panel if instrumented
2. List DLQ candidates: `GET /compensation-actions/dlq`
3. Check artifact storage connectivity (MinIO/S3)

**Mitigation:**
1. If artifact storage is unavailable: artifacts are buffered locally
2. Once storage recovers: re-trigger capture via retry endpoint
3. If quarantine fails permanently: manual cleanup required

**Recovery:**
- Artifacts are not lost — they remain in local buffer until quarantine succeeds
- Monitor `intent_api_side_effect_captured_total` rate to confirm recovery

---

### RB9. Compensation timeout

**Symptoms:**
- Compensation execution never completes
- `GET /compensation-actions/{id}` shows status = "executed" but side effects not reversed

**Diagnosis:**
1. Check `intent_api_compensation_execution_total` panel
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

**Symptoms:**
- DLQ depth alert fires (`intent_api_dlq_messages_current > 10`)
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
4. Check application metrics in Grafana (`intent_api_dlq_messages_current`, `intent_api_dlq_replay_total`, `intent_api_dlq_replay_failures_total`)

**Prevention:**
- Monitor `intent_api_dlq_messages_current` metric (alert threshold: > 10 messages)
- Monitor `intent_api_dlq_message_age_seconds` (alert threshold: > 3600s = 1 hour)
- Review DLQ messages weekly if volume is non-zero
- Set `max_deliver` appropriately per message type (see `14-dlq-retry-design.md`)

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
| CompensationDLQCandidatesElevated | Critical | Check DLQ, manual intervention likely needed |
| DLQDepthHigh | Warning | **Designed/deferred** — alert rule not deployed; see doc 14 DLQ design |
| DLQMessageStale | Critical | **Designed/deferred** — alert rule not deployed; see doc 14 DLQ design |
| PreviewPathBurnRate1h/6h/3d | Warning | Monitor burn rate windows; prepare incident if 1h persists |
| ApplyPathBurnRate1h/6h/3d | Critical | Open incident, prioritize fix — check which window is firing |
