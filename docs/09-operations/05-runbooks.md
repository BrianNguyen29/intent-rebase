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

## Batch 2 Runbooks (Phase 3 Batch 2 Slice 3)

### RB6. Rebase stuck / no response

**Symptoms:**
- Rebase preview or apply endpoint times out or returns no response
- Latency histogram shows p95 > SLO threshold (10s for preview, 60s for apply)

**Diagnosis:**
1. Check `intent_api_rebase_preview_duration_seconds` and `intent_api_rebase_apply_duration_seconds` panels in Grafana
2. Check rebase-engine logs for compute_diff or planner errors
3. Verify graph-service connectivity and node count (large graphs > 1000 nodes can cause timeout)

**Mitigation:**
1. If specific intent is stuck: cancel the workflow via Temporal console
2. If graph is large: enable preview-only mode for affected tenant
3. Scale rebase-engine workers if CPU-bound
4. If adapter is degraded: follow RB3 (Runtime adapter failing apply)

**Recovery:**
- Stuck rebase operations cannot be automatically retried
- Operator may need to replay from a known-good checkpoint

---

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
- Alert fires: `PreviewPathFastBurn` or `ApplyPathFastBurn`
- Error budget panel shows rapid consumption

**Diagnosis:**
1. Check which SLO is being breached (preview availability vs apply availability)
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
| PreviewPathFastBurn | Warning | Monitor, prepare incident if continues |
| ApplyPathFastBurn | Critical | Open incident, prioritize fix |
