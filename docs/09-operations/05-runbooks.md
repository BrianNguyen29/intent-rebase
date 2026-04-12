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

## RB6. Error Budget Exceeded (Phase 3 Batch 2)

### Symptoms
- Alert: `ErrorBudgetExhausted` or `ErrorBudgetDepleted` firing
- Error rate elevated for one or more SLOs

### Diagnosis
1. Check Grafana dashboard `intent-rebase-error-budget`
2. Identify which SLO is burning budget fastest
3. Check recent incidents: `git log --since="24 hours ago"`

### Response

**If budget 20-50% remaining:**
1. Acknowledge alert
2. Investigate root cause of elevated errors
3. Implement fix if known
4. Monitor for 1 hour

**If budget < 20% remaining (critical):**
1. Page on-call SRE
2. Open incident
3. Prioritize fix - consider rollback
4. Notify tenant if SLO breach imminent

**If budget depleted:**
1. Incident is declared
2. All hands - focus on recovery
3. Consider temporary SLO relaxation
4. Post-mortem required

### Prevention
- Monitor burn rate
- Set up warning alerts at 50% budget
- Review error budget quarterly

## RB7. Approval Wait Time Elevated (Phase 3 Batch 2)

### Symptoms
- Alert: `ApprovalWaitLatencyWarning` or `ApprovalWaitLatencyCritical` firing
- p95 approval wait > 30 minutes (warning) or > 60 minutes (critical)

### Diagnosis
1. Check Grafana dashboard `intent-rebase-slo`
2. Identify affected intent IDs
3. Check approval service logs for bottlenecks

### Response
1. Acknowledge alert
2. Review approval backlog in API: `GET /intents/{intent_id}/versions`
3. Contact approvers if workflow stalled
4. Check for deadlocks in approval state machine

## RB8. Rebase Latency Elevated (Phase 3 Batch 2)

### Symptoms
- Alert: `RebasePreviewLatencyWarning` or `RebaseApplyLatencyCritical` firing
- p95 rebase time > 10s (preview warning) or > 20s (preview critical)
- p95 rebase apply > 60s (warning) or > 120s (critical)

### Diagnosis
1. Check Grafana dashboard `intent-rebase-slo`
2. Identify latency percentile affected (p50, p95, p99)
3. Check graph traversal metrics
4. Review rebase engine logs

### Response
1. Identify if issue is transient or sustained
2. Check database query performance
3. Review graph service health
4. Consider scaling rebase engine workers

## RB9. Compensation Execution Failures (Phase 3 Batch 1)

### Symptoms
- Alert: `CompensationExecutionFailureRateWarning` or `CompensationExecutionFailureRateCritical`
- Failure rate > 1% (warning) or > 5% (critical)

### Diagnosis
1. Check compensation service logs
2. Identify failure patterns (strategy type, error class)
3. Review side effect ledger for affected intents

### Response

**Warning level:**
1. Monitor for 15 minutes
2. Check if failures are retriable
3. Review compensation action history

**Critical level:**
1. Page on-call if sustained > 5 minutes
2. Identify affected tenants
3. Consider pausing automatic execution
4. Manual review of failed actions
