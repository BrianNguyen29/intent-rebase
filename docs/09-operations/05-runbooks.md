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
