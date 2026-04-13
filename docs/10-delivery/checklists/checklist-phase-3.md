# Phase 3 — Compensation + Production Hardening Checklist

**Exit Gate:** Phase 3 exit gate khi tất cả items checked và có evidence.  
**Prerequisite:** Phase 2b exit gate passed. Phase 2b scope includes: runtime adapter external implementation, apply endpoint, risk classification, graph update, replay API, event streaming. Phase 3 Batch 0 (hardening planning and scaffold prep) may proceed in parallel while Phase 2b is in progress — see [05-phase-3-hardening.md](../05-phase-3-hardening.md) for batch structure.

**Trạng thái:** `BATCH 0 COMPLETE, BATCH 1 LARGELY DELIVERED` — Batch 0 code scaffolds and planning items complete. Batch 1 side effect ledger, compensation action CRUD + APIs, batch orchestration, policy gate, orchestration dashboard, orchestration coordination view, dry-run planner, and single-shot runtime (HTTP + CLI) all delivered. Formal planner/executor/retry/rollback record remains gated on Phase 2b exit. See [05-phase-3-hardening.md](../05-phase-3-hardening.md) for the current execution split.  
**Phase:** Phase 3  
**Target Duration:** 6–10 tuần

---

## Batch 0 Progress Snapshot

```
[x] Batch 0 scaffold: compensation service package structure created
    Evidence:
    - Code: crates/compensation-service/Cargo.toml
    - Code: crates/compensation-service/src/lib.rs
    - Code: crates/compensation-service/src/side_effect.rs
    - Code: crates/compensation-service/src/compensation_action.rs
    - Tests: cargo test -p compensation-service --all-features

[x] Batch 0 scaffold: forensic service package structure created
    Evidence:
    - Code: crates/forensic-service/Cargo.toml
    - Code: crates/forensic-service/src/lib.rs
    - Code: crates/forensic-service/src/bundle.rs
    - Code: crates/forensic-service/src/bundle_contents.rs
    - Tests: cargo test -p forensic-service --all-features

[x] Batch 0 groundwork: Phase 3 audit taxonomy extended for compensation/forensic flows
    Evidence:
    - Code: crates/intent-rebase-types/src/audit.rs
    - Code: crates/intent-rebase-types/src/audit_repo.rs
    - Scope: additive event taxonomy only; no producer/consumer wiring yet

[~] Batch 0 planning/admin items partially prepared
    Evidence:
    - Dependency audit artifact: ../07-phase-3-dependency-audit.md
    - Phase 2b security input artifact: ../08-phase-2b-security-findings-input.md
    - Provisional SLO prep: ../../09-operations/04-sre-and-slos.md
    - Ownership/sign-off still awaits named assignees and external confirmation
    - Final SRE/security/compliance sign-off remains open
    - Tracking plan: ../06-phase-3-batch-0-execution.md
```

---

## 1. Side Effect Ledger

```
[x] Side effect model (effect_id, intent_id, intent_version, effect_type, target, timestamp, tenant_id)
    Evidence:
    - PR merged: <link>
    - Code: crates/compensation-service/src/side_effect.rs (tenant_id added)
    - Code: crates/compensation-service/src/side_effect_repo.rs (persist/query groundwork only)
    - Schema: infrastructure/migrations/010_create_side_effects_ledger.sql
    - Note: This slice delivers persistence + repository groundwork only. Capture-on-write, API, idempotency enforcement, and rollback records remain open below.

[~] Side effect capture on artifact-producing operations (Phase 3 Batch 1 groundwork)
    Evidence:
    - Code: crates/intent-rebase-types/src/graph.rs (SideEffectCaptureContext added)
    - Code: crates/graph-service/src/lib.rs (side_effect_context field on ArtifactIngestRequest, documentation updated)
    - Code: crates/intent-api/src/lib.rs (ingest_artifact endpoint with optional side effect capture, 16 validation tests for side_effect_context)
    - Tests: cargo test -p graph-service --all-features (78 tests pass), cargo test -p intent-api --all-features (67 tests pass)
    - Note: Delivered capture path is artifact-ingest only (via POST /v1/graph/artifacts with side_effect_context). Broader capture across other artifact-producing operations remains open and requires artifact-service integration.

[x] Side effect query API for compensation planning (Phase 3 Batch 1 groundwork)
    Evidence:
    - PR merged: <link>
    - Code: crates/compensation-service/src/side_effect_service.rs (service facade with list_side_effects_by_intent)
    - Code: crates/intent-api/src/lib.rs (GET /intents/{intent_id}/side-effects endpoint)
    - Tests: cargo test -p compensation-service --all-features (30 tests pass), cargo test -p intent-api --all-features (67 tests pass)

[x] Side effect idempotency keys (Phase 3 Batch 1 groundwork - via service facade)
    Evidence:
    - Code: crates/compensation-service/src/side_effect_service.rs (atomic record_side_effect_with_idempotency)
    - Code: crates/compensation-service/src/side_effect_repo.rs (tenant-scoped get_or_create_idempotent)
    - Tests: cargo test -p compensation-service --all-features (idempotency tests pass, including concurrent duplicate protection)
    - Note: Atomic tenant-scoped idempotency is now implemented in the service/repository path. Broader artifact-service coverage remains open.

[x] Side effect rollback record (compensation applied, compensation result)
    Evidence:
    - Code: crates/compensation-service/src/rollback_record.rs
    - Tests: cargo test -p compensation-service --all-features
```

---

## 2. Compensation Engine

```
[~] Compensation action model and repository (Batch 1 scaffold)
    Evidence:
    - Code: crates/compensation-service/src/compensation_action.rs (model)
    - Code: crates/compensation-service/src/compensation_action_repo.rs (trait + InMemory/SQL implementations)
    - Schema: infrastructure/migrations/011_create_compensation_actions.sql
    - Tests: cargo test -p compensation-service --all-features
    - Note: Planner delivered (Phase 3 Batch 1 bounded slice; class-based strategy routing; S2 routes to CounterAction+SemiAutomatic; fail-closed on unsupported strategy classes)

[x] Compensation actions query API (Phase 3 Batch 1 bounded read-only slice)
    Evidence:
    - Code: crates/intent-api/src/lib.rs (GET /intents/{intent_id}/compensation-actions endpoint)
    - Code: crates/compensation-service/src/compensation_action_service.rs (list_by_intent method)
    - Tests: cargo test -p intent-api --all-features (73 tests pass)
    - Note: This endpoint is READ-ONLY - does not trigger compensation execution.

[x] Compensation action approve API (Phase 3 Batch 1 bounded execution slice)
    Evidence:
    - Code: crates/intent-api/src/lib.rs (POST /compensation-actions/{action_id}/approve)
    - Code: crates/compensation-service/src/compensation_action_service.rs (approve_action method)
    - Tests: cargo test -p compensation-service --all-features (86 tests pass), cargo test -p intent-api --all-features (73 tests pass)
    - Note: Transitions Pending → Approved with optimistic locking; fails closed on illegal transitions.

[x] Compensation action waive API (Phase 3 Batch 1 bounded execution slice)
    Evidence:
    - Code: crates/intent-api/src/lib.rs (POST /compensation-actions/{action_id}/waive)
    - Code: crates/compensation-service/src/compensation_action_service.rs (waive_action method)
    - Tests: cargo test -p compensation-service --all-features (86 tests pass), cargo test -p intent-api --all-features (73 tests pass)
    - Note: Transitions Pending → Waived with optimistic locking; fails closed on illegal transitions.

[x] Compensation action execute API (Phase 3 Batch 1 bounded execution slice)
    Evidence:
    - Code: crates/intent-api/src/lib.rs (POST /compensation-actions/{action_id}/execute)
    - Code: crates/compensation-service/src/compensation_action_service.rs (execute_action method)
    - Tests: cargo test -p compensation-service --all-features (86 tests pass), cargo test -p intent-api --all-features (73 tests pass)
    - Note: Bounded RollbackExecutor for Rollback+Automatic path only. Three additional bounded executors delivered: CounterActionExecutor (CounterAction+SemiAutomatic), FollowupNoticeExecutor (FollowupNotice+ManualOnly), EscalationExecutor (Escalation+NotPossible). All four executors acknowledge against side effect ledger; fail-closed on non-matching strategy/feasibility combos. S2 alignment resolved: S2ExternalReversible routes to CounterAction+SemiAutomatic.

[x] Compensation action DLQ query API (Phase 3 Batch 1 bounded manual retry slice)
    Evidence:
    - Code: crates/intent-api/src/lib.rs (GET /compensation-actions/dlq)
    - Code: crates/compensation-service/src/compensation_action_service.rs (list_dlq_candidates, get_dlq_candidate_count)
    - Tests: cargo test -p compensation-service --all-features (DLQ derivation tests pass)
    - Note: Derived DLQ condition from existing data (Failed + exhausted budget OR non-retryable error). No DLQ table.

[x] Compensation action batch candidates API (Phase 3 Batch 1 bounded read-only batch candidate queue slice)
    Evidence:
    - Code: crates/intent-api/src/lib.rs (GET /compensation-actions/batch-candidates)
    - Code: crates/intent-api/src/lib.rs (ListBatchCandidatesResponse, ListBatchCandidatesQuery, BatchCandidatesSummary DTOs)
    - Code: crates/intent-api/src/lib.rs (list_batch_candidates handler)
    - Code: crates/compensation-service/src/lib.rs (BatchCandidates re-export)
    - Code: crates/compensation-service/src/compensation_action_service.rs (list_batch_candidates method)
    - Code: crates/compensation-service/src/compensation_action_service.rs (BatchCandidates struct)
    - OpenAPI: docs/04-api/openapi.yaml (endpoint path and schema definitions)
    - Tests: cargo test -p compensation-service --all-features (batch candidates tests pass)
    - Note: Read-only endpoint returning four categories (pending approval, approved auto-executable, retryable failed, DLQ). No execution, orchestration, or policy gate.

[x] Compensation action reapprove API (Phase 3 Batch 1 bounded manual retry slice)
    Evidence:
    - Code: crates/intent-api/src/lib.rs (POST /compensation-actions/{action_id}/reapprove)
    - Code: crates/compensation-service/src/compensation_action_service.rs (reapprove_action method)
    - Code: crates/compensation-service/src/compensation_action_repo.rs (reapprove repository method)
    - Tests: cargo test -p compensation-service --all-features (reapprove tests pass)
    - Note: Manual retry gate implemented with fail-closed policy. Only retryable errors AND remaining budget allow reapproval.

[x] Intent orchestration dashboard API (Phase 3 Batch 1 bounded read-only slice)
    Evidence:
    - Code: crates/intent-api/src/lib.rs (GET /intents/{intent_id}/orchestration-dashboard)
    - Code: crates/intent-api/src/lib.rs (OrchestrationDashboardResponse, SideEffectSummary, CompensationActionSummary, CompensationActionStatusCounts DTOs)
    - Code: crates/intent-api/src/lib.rs (get_orchestration_dashboard handler with summary derivation)
    - OpenAPI: docs/04-api/openapi.yaml (endpoint path and schema definitions)
    - Tests: cargo test -p intent-api --all-features (7 dashboard tests pass)
    - Note: Bounded read-only endpoint. All summary fields derived from persisted data via existing service query helpers. No batch execution or orchestration engine claims.

[x] Compensation action policy gate evaluation API (Phase 3 Batch 1 bounded read-only slice)
    Evidence:
    - Code: crates/intent-api/src/lib.rs (GET /compensation-actions/policy-gate and GET /intents/{intent_id}/compensation-policy-gate)
    - Code: crates/intent-api/src/lib.rs (CompensationPolicyGateQuery, IntentCompensationPolicyGateQuery, CompensationPolicyGateResponse, PolicyGateEvaluationResponse, PolicyGateMetadataResponse, RiskMetadataResponse, ErrorClassificationResponse DTOs)
    - Code: crates/intent-api/src/lib.rs (get_compensation_policy_gate and get_intent_compensation_policy_gate handlers)
    - Code: crates/intent-api/src/lib.rs (format_strategy_severity, format_retry_exhaustion_risk, format_feasibility_risk, format_error_severity formatters)
    - Code: crates/compensation-service/src/compensation_action_service.rs (PolicyGateStatus enum, PolicyGateEvaluation struct, PolicyGateMetadata struct, RiskMetadata struct, StrategySeverity enum, RetryExhaustionRisk enum, FeasibilityRisk enum, ErrorSeverity enum, ErrorClassification struct)
    - Code: crates/compensation-service/src/compensation_action_service.rs (evaluate_policy_gates, evaluate_policy_gates_for_intent, evaluate_single_action, compute_gate_status, compute_gate_reason, compute_policy_metadata, compute_risk_metadata methods)
    - Code: crates/compensation-service/src/lib.rs (RiskMetadata, ErrorClassification, ErrorSeverity, FeasibilityRisk, RetryExhaustionRisk, StrategySeverity re-exports)
    - OpenAPI: docs/04-api/openapi.yaml (/compensation-actions/policy-gate and /intents/{intent_id}/compensation-policy-gate endpoint definitions with request/response schemas)
    - OpenAPI: docs/04-api/openapi.yaml (CompensationPolicyGateResponse, PolicyGateEvaluationResponse, PolicyGateMetadataResponse, RiskMetadataResponse, PolicyGateSummaryResponse schema definitions)
    - Tests: cargo test -p compensation-service --all-features (141 tests pass), cargo test -p intent-api --all-features (80 tests pass)
    - Note: Bounded read-only endpoint. Gate status derived from existing fields (status, feasibility, attempt_count, max_retries, error_code). No new policy engine. Canonical statuses: eligible | blocked | manual_review_required. Risk metadata includes strategy_severity, retry_exhaustion_risk, feasibility_risk, error_severity, retry_budget_remaining, error_classification, is_terminal, requires_manual_intervention.

[x] Orchestration coordination status API (Phase 3 Batch 1 bounded read-only orchestration coordination view)
    Evidence:
    - Code: crates/compensation-service/src/compensation_action_service.rs (CoordinationStatus enum: ready, awaiting_policy, awaiting_manual_review, blocked, terminal)
    - Code: crates/compensation-service/src/compensation_action_service.rs (CoordinationRecord, CoordinationSummary, CoordinationResult structs)
    - Code: crates/compensation-service/src/compensation_action_service.rs (CoordinationStatus::from_compensation_action, CoordinationRecord::from_action methods)
    - Code: crates/compensation-service/src/compensation_action_service.rs (evaluate_coordination_status, evaluate_coordination_status_for_intent, evaluate_coordination_from_actions methods)
    - Code: crates/compensation-service/src/lib.rs (CoordinationRecord, CoordinationResult, CoordinationStatus, CoordinationSummary re-exports)
    - Code: crates/intent-api/src/lib.rs (GET /compensation-actions/orchestration-coordination and GET /intents/{intent_id}/orchestration-coordination)
    - Code: crates/intent-api/src/lib.rs (OrchestrationCoordinationQuery, IntentOrchestrationCoordinationQuery, OrchestrationCoordinationResponse, CoordinationRecordResponse, CoordinationSummaryResponse DTOs)
    - Code: crates/intent-api/src/lib.rs (get_orchestration_coordination and get_intent_orchestration_coordination handlers)
    - Code: crates/intent-api/src/lib.rs (format_coordination_status formatter)
    - OpenAPI: docs/04-api/openapi.yaml (/compensation-actions/orchestration-coordination and /intents/{intent_id}/orchestration-coordination endpoint definitions)
    - OpenAPI: docs/04-api/openapi.yaml (OrchestrationCoordinationResponse, CoordinationRecordResponse, CoordinationSummaryResponse schema definitions)
    - OpenAPI: docs/04-api/openapi.yaml (updated API description with new endpoints)
    - Note: Bounded read-only orchestration coordination view. Canonical statuses: ready | awaiting_policy | awaiting_manual_review | blocked | terminal. Per-item records include coordination_status, coordination_reason, and action details. Summary counts: ready_count, awaiting_policy_count, awaiting_manual_review_count, blocked_count, terminal_count, dlq_candidate_count, auto_executable_count. No new orchestration engine - all fields derive from existing CompensationAction fields at query time.

[x] Orchestration dry-run planner API (Phase 3 Batch 1 bounded manual orchestration dry-run slice)
    Evidence:
    - Code: crates/compensation-service/src/compensation_action_service.rs (OrchestrationAction enum: Approve, Reapprove, Execute, NoAction)
    - Code: crates/compensation-service/src/compensation_action_service.rs (OrchestrationActionProposal, OrchestrationDryRunResult, OrchestrationDryRunSummary structs)
    - Code: crates/compensation-service/src/compensation_action_service.rs (plan_orchestration_actions, compute_action_proposal methods)
    - Code: crates/compensation-service/src/lib.rs (OrchestrationAction, OrchestrationActionProposal, OrchestrationDryRunResult, OrchestrationDryRunSummary re-exports)
    - Code: crates/intent-api/src/lib.rs (POST /compensation-actions/orchestration-dry-run)
    - Code: crates/intent-api/src/lib.rs (OrchestrationDryRunRequest, OrchestrationDryRunResponse, OrchestrationActionProposalResponse, OrchestrationDryRunSummaryResponse DTOs)
    - Code: crates/intent-api/src/lib.rs (orchestration_dry_run handler)
    - OpenAPI: docs/04-api/openapi.yaml (/compensation-actions/orchestration-dry-run endpoint definition)
    - OpenAPI: docs/04-api/openapi.yaml (OrchestrationDryRunRequest, OrchestrationDryRunResponse, OrchestrationActionProposalResponse, OrchestrationDryRunSummaryResponse schema definitions)
    - Tests: cargo test -p compensation-service --all-features (150 tests pass), cargo test -p intent-api --all-features (80 tests pass)
    - Note: Bounded READ-ONLY dry-run. Returns per-item proposed action (approve | reapprove | execute | no_action) + reason. No execution, no background worker, no queue claiming. Tenant isolation enforced.

[x] Batch approve API (Phase 3 Batch 1 bounded manual orchestration slice)
    Evidence:
    - Code: crates/compensation-service/src/compensation_action_service.rs (BatchOrchestrationResult, BatchItemOutcome, BatchOrchestrationSummary structs)
    - Code: crates/compensation-service/src/compensation_action_service.rs (batch_approve method)
    - Code: crates/compensation-service/src/lib.rs (BatchOrchestrationResult, BatchItemOutcome, BatchOrchestrationSummary re-exports)
    - Code: crates/intent-api/src/lib.rs (POST /compensation-actions/batch-approve)
    - Code: crates/intent-api/src/lib.rs (BatchOrchestrationRequest, BatchOrchestrationResponse, BatchItemOutcomeResponse, BatchOrchestrationSummaryResponse DTOs)
    - Code: crates/intent-api/src/lib.rs (batch_approve_compensation_actions handler)
    - OpenAPI: docs/04-api/openapi.yaml (/compensation-actions/batch-approve endpoint definition)
    - OpenAPI: docs/04-api/openapi.yaml (BatchOrchestrationRequest, BatchOrchestrationResponse, BatchItemOutcomeResponse, BatchOrchestrationSummaryResponse schema definitions)
    - Tests: cargo test -p compensation-service --all-features (150 tests pass), cargo test -p intent-api --all-features (80 tests pass)
    - Note: Bounded batch approve for explicit action IDs with partial-success semantics. No background worker, no queue claiming. Uses existing approve_action service method with tenant isolation.

[x] Batch reapprove API (Phase 3 Batch 1 bounded manual orchestration slice)
    Evidence:
    - Code: crates/compensation-service/src/compensation_action_service.rs (batch_reapprove method)
    - Code: crates/intent-api/src/lib.rs (POST /compensation-actions/batch-reapprove)
    - Code: crates/intent-api/src/lib.rs (batch_reapprove_compensation_actions handler)
    - OpenAPI: docs/04-api/openapi.yaml (/compensation-actions/batch-reapprove endpoint definition)
    - Tests: cargo test -p compensation-service --all-features (150 tests pass), cargo test -p intent-api --all-features (80 tests pass)
    - Note: Bounded batch reapprove for explicit action IDs with partial-success semantics. No background worker, no queue claiming. Uses existing reapprove_action service method with tenant isolation.

[x] Batch execute API (Phase 3 Batch 1 bounded manual orchestration slice)
    Evidence:
    - Code: crates/compensation-service/src/compensation_action_service.rs (batch_execute method)
    - Code: crates/intent-api/src/lib.rs (POST /compensation-actions/batch-execute)
    - Code: crates/intent-api/src/lib.rs (batch_execute_compensation_actions handler)
    - OpenAPI: docs/04-api/openapi.yaml (/compensation-actions/batch-execute endpoint definition)
    - Tests: cargo test -p compensation-service --all-features (150 tests pass), cargo test -p intent-api --all-features (80 tests pass)
    - Note: Bounded batch execute for explicit action IDs with partial-success semantics. No background worker, no queue claiming. Uses existing execute_action service method with tenant isolation.

[x] Single-shot orchestration runtime - HTTP POST /compensation-actions/runs (Phase 3 Batch 1 bounded single-shot HTTP slice)
    Evidence:
    - Code: crates/compensation-service/src/orchestration_runtime.rs (OrchestrationRuntime struct with execute_run method)
    - Code: crates/compensation-service/src/orchestration_run.rs (OrchestrationRun, RunStatus, OrchestrationActionDecision, RunItemResult models)
    - Code: crates/compensation-service/src/orchestration_run_repo.rs (OrchestrationRunRepository trait + InMemory/Sqlx implementations)
    - Code: crates/intent-api/src/lib.rs (POST /compensation-actions/runs, create_orchestration_run handler, HTTP 202 Accepted)
    - Code: crates/intent-api/src/lib.rs (CreateOrchestrationRunRequest, OrchestrationRunResponse, RunItemResultResponse DTOs)
    - Code: crates/intent-api/src/lib.rs (AppState.orchestration_runtime field + routing)
    - OpenAPI: docs/04-api/openapi.yaml (/compensation-actions/runs POST endpoint definition)
    - OpenAPI: docs/04-api/openapi.yaml (CreateOrchestrationRunRequest, OrchestrationRunResponse, RunItemResultResponse schema definitions)
    - Tests: cargo test -p compensation-service --all-features (166 tests pass including orchestration_runtime tests), cargo test -p intent-api --all-features (80 tests pass)
    - Note: Bounded single-shot HTTP. Auto-decides approve|reapprove|execute|skip per action using existing service methods. HTTP 202 returns persisted run handle. No queue polling, no distributed claiming/locking, no scheduler.

[x] Single-shot orchestration runtime - HTTP GET /compensation-actions/runs/{run_id} (Phase 3 Batch 1 bounded read surface)
    Evidence:
    - Code: crates/intent-api/src/lib.rs (GET /compensation-actions/runs/{run_id}, get_orchestration_run handler)
    - Code: crates/intent-api/src/lib.rs (OrchestrationRunQuery for tenant_id parameter)
    - Code: crates/compensation-service/src/orchestration_runtime.rs (OrchestrationRuntime.get_run method)
    - Code: crates/compensation-service/src/orchestration_run_repo.rs (OrchestrationRunRepository.get_run method)
    - OpenAPI: docs/04-api/openapi.yaml (/compensation-actions/runs/{run_id} GET endpoint definition)
    - Tests: cargo test -p compensation-service --all-features (166 tests pass), cargo test -p intent-api --all-features (80 tests pass)
    - Note: Bounded read surface for persisted run handles. Tenant isolation verified. Returns run status, counts, and per-item results.

[x] Single-shot orchestration runtime - CLI sync (Phase 3 Batch 1 bounded CLI slice)
    Evidence:
    - Code: crates/intent-cli/Cargo.toml (new crate with ureq, clap dependencies)
    - Code: crates/intent-cli/src/main.rs (CLI with run and get-run subcommands)
    - Code: crates/intent-cli/src/main.rs (run_orchestration function: POST /compensation-actions/runs)
    - Code: crates/intent-cli/src/main.rs (get_run function: GET /compensation-actions/runs/{run_id})
    - Tests: cargo check -p intent-cli (compiles successfully)
    - Note: Bounded CLI sync for explicit action IDs. Uses ureq for HTTP transport. Auto-decides approve|reapprove|execute|skip per action. Single-shot only - one run per invocation.
```

---

## 3. SRE & Observability

**P2-S2 Bounded Slice — Items 2-1, 2-2, 2-3 Delivered**
**P2-S3 Bounded Slice — Item 2-4 (bounded distributed tracing slice) Delivered**

```
[x] SLO definitions (intent processing latency, rebase latency, approval wait time) ✅ P2-S2
    Evidence:
    - PR merged: <link>
    - Doc: ../../09-operations/04-sre-and-slos.md (updated)
    - Dashboard: Grafana SLO dashboard (intent-rebase-slo)
    - Metrics defined: crates/intent-api/src/metrics.rs

[x] Alerting rules (warning, critical thresholds) ✅ P2-S2
    Evidence:
    - PR merged: <link>
    - Alert rules: infrastructure/local/prometheus/rules.yml
    - Alertmanager config: infrastructure/local/alertmanager/alertmanager.yml
    - Grafana datasource: infrastructure/local/grafana/provisioning/datasources/datasources.yml

[x] Error budget tracking ✅ P2-S2
    Evidence:
    - Dashboard: infrastructure/local/grafana/provisioning/dashboards/error-budget-dashboard.json
    - Runbook: ../../09-operations/05-runbooks.md (RB6, RB7, RB8, RB9 added)
    - Metrics: intent_api_error_budget_remaining gauge per SLO

[x] Distributed tracing across all services (bounded P2-S3 slice — trace context propagated via existing AuditEvent trace_id/span_id fields) ✅ P2-S3
    Evidence:
    - Code: crates/intent-api/src/trace_context.rs (trace context extraction helper)
    - Code: crates/intent-rebase-types/src/audit_repo.rs (record_*_with_trace methods)
    - Code: crates/intent-api/src/lib.rs (trace context propagation in audit calls)
    - Instrumented: RebaseApplied, RebaseApplyBlocked, ApprovalGranted, ApprovalRevoked, ApprovalExpired, ReplayInitiated events carry trace_id/span_id
    - Note: Full OTLP/cross-service trace propagation is P2-S4+ scope

[~] Performance benchmarks: rebase latency p50/p95/p99 (local baseline captured; CI-averaged targets and load testing gated on P2 completion)
    Evidence:
    - CI job: .github/workflows/ci.yml#bench (runs cargo bench -p rebase-engine, uploads criterion reports as artifacts)
    - Harness: crates/rebase-engine/benches/diff_latency.rs (criterion-based, harness=false)
    - Baseline results: docs/11-quality/benchmark-baseline-results.md (local baseline measured April 2026: p50 range 3.78–6.09 µs)
    - Note: This slice delivers benchmark harness infrastructure and local baseline numbers. Actual CI-averaged p50/p95/p99 targets and production load testing (k6/Artillery) remain gated on P2 full completion.

[ ] Runbooks for common failure scenarios
    Evidence:
    - Doc: ../../09-operations/05-runbooks.md (updated with Phase 3 scenarios)
    - Runbooks: rebase-stuck, approval-backlog, artifact-quarantine-fail, compensation-timeout
```

---

## 4. Tenant Isolation Hardening

```
[x] Tenant isolation verification tests — P3-S1 SLICE ✅
    Evidence:
    - PR merged: <link>
    - Tests: cross-tenant access attempts blocked ✅
    - Tests: data leakage tests (tenant A cannot see tenant B data) ✅
    - Tests: intent-api approval-request endpoints (list, approve, reject, expire)
    - Tests: orchestration dashboard tenant isolation

[x] Resource quota enforcement (intents per tenant, artifacts per tenant) — P3-S2 bounded slice
    Evidence:
    - Code: crates/intent-rebase-types/src/quota.rs (QuotaService, InMemoryQuotaRepository, QuotaRepository trait)
    - Code: crates/intent-rebase-types/src/error.rs (QuotaExceeded error variant)
    - Code: crates/intent-api/src/lib.rs (quota_service field in AppState, quota checks on create_intent and ingest_artifact)
    - Code: crates/intent-service/src/sqlx_repository.rs (tenant_id from request)
    - Code: crates/intent-rebase-types/src/intent.rs (tenant_id field on CreateIntentRequest)
    - Tests: cargo test -p intent-rebase-types --all-features (quota tests pass)
    - Default limits: 10000 intents, 100000 artifacts per tenant

[x] Tenant-specific rule pack isolation (P3-S3 bounded slice)
    Evidence:
    - Code: crates/rebase-engine/src/rule_pack.rs (RulePackVersion derives Hash)
    - Code: crates/rebase-engine/src/rule_pack_registry.rs (TenantRulePackRepository trait + InMemory impl)
    - Code: crates/rebase-engine/src/lib.rs (exports from rule_pack_registry module)
    - Tests: cargo test -p rebase-engine --all-features (141 tests pass, including 8 tenant isolation tests)
    - Doc: docs/14-governance/08-tenant-isolation.md (Layer 4 rule pack registry isolation documented)
    - Note: Bounded slice delivers registry primitives only. Full upload/management API, S3 integration, and rule evaluation engine rewiring remain out of scope for this slice.

[x] Tenant audit log separation (P3-S4 bounded slice)
    Evidence:
    - Code: crates/intent-rebase-types/src/audit_repo.rs (get_audit_event method added to AuditRepository trait)
    - Code: crates/intent-api/src/lib.rs (GET /audit/events and GET /audit/events/{event_id} endpoints)
    - Code: crates/intent-api/src/lib.rs (6 cross-tenant isolation tests passing)
    - Tests: cargo test -p intent-api --all-features (cross-tenant audit tests pass)
    - Doc: docs/14-governance/08-tenant-isolation.md (Layer 5 audit query API isolation documented)
    - Note: This slice delivers tenant-scoped audit query API only. S3 cold storage and archival are Phase 4+ scope.

[ ] Data residency: tenant data stays in assigned region
    Evidence:
    - Doc: ../../08-security/01-threat-model.md (updated)
    - Code: multi-region routing

[x] Tenant service scaffold (P3-S5 bounded slice — onboarding groundwork only)
    Evidence:
    - Code: crates/tenant-service/Cargo.toml (new crate)
    - Code: crates/tenant-service/src/lib.rs (service scaffold with re-exports)
    - Code: crates/tenant-service/src/tenant.rs (Tenant model, TenantStatus, TenantRegion)
    - Code: crates/tenant-service/src/tenant_repo.rs (TenantRepository trait + InMemory impl)
    - Code: crates/intent-rebase-types/src/error.rs (TenantNotFound, TenantNotFoundBySlug errors)
    - Tests: cargo test -p tenant-service --all-features (15 tests pass)
    - Doc: docs/14-governance/08-tenant-isolation.md (Layer 6 tenant service documented)
    - Note: Bounded slice delivers tenant model + repository scaffold only. SQL persistence, API endpoints, residency routing, and offboarding deletion are future phase scope.

[~] Tenant onboarding procedures documented (P3-S5 bounded slice — scaffold-level runbook only)
    Evidence:
    - Doc: docs/09-operations/06-tenant-onboarding.md (new — runbook skeleton)
    - Note: Delivers procedure skeleton/runner documentation for onboarding workflow. Full API implementation, S3 bucket provisioning, NATS account creation, and RBAC setup are future phase scope.
```

---

## 5. Forensic Replay Bundle

**P4 Bounded Slice — Items 5-1, 5-2, 5-5 Delivered**

```
[x] Forensic bundle model + status tracking (P4 bounded slice)
    Evidence:
    - Code: crates/forensic-service/src/bundle.rs (BundleStatus enum with Pending/Generating/Ready/Failed)
    - Code: crates/forensic-service/src/bundle_contents.rs
    - Code: crates/forensic-service/src/bundle_repo.rs (BundleRepository trait + InMemoryBundleRepository impl)
    - Code: crates/intent-rebase-types/src/error.rs (ForensicBundleNotFound, InvalidForensicBundleStatusTransition)
    - Tests: cargo test -p forensic-service --all-features (26 tests pass)
    - Note: Bounded slice delivers status tracking primitives and in-memory repository only. S3 storage, generation API, integrity verification, and replay are Phase 4 scope.

[x] Bundle content collection primitives — P4 bounded slice (content collection + integrity hashing)
    Evidence:
    - Code: crates/forensic-service/src/bundle_hasher.rs (SHA-256 hashing, BundleIntegrityHash, ContentSectionHash, section hash input types)
    - Code: crates/forensic-service/src/bundle_generator.rs (BundleGeneratorService, GenerateBundleRequest, BundleGenerationResult)
    - Tests: cargo test -p forensic-service --all-features (46 tests pass — deterministic hashing, content counts, tamper detection)
    - Doc: docs/14-governance/10-forensic-bundle.md (updated scope marker)
    - Note: Bounded slice delivers content collection types (IntentVersionsForHash, ArtifactsForHash, ApprovalsForHash, AuditEventsForHash, PolicySnapshotsForHash) and deterministic SHA-256 integrity hashing. No S3 storage, no generation API, no replay.

[x] Bundle integrity verification (hash chain) — P4 bounded slice
    Evidence:
    - Code: crates/forensic-service/src/bundle_hasher.rs (verify_bundle_integrity function, IntegrityVerificationFailure)
    - Code: crates/forensic-service/src/bundle_generator.rs (BundleGeneratorService::verify_integrity method)
    - Tests: verify_bundle_integrity passes on clean content, fails on tampered content
    - Note: Verifies all 5 section hashes (intent_versions, artifacts, approvals, audit_events, policy_snapshots) against recorded integrity hash.

[ ] Forensic bundle model (`bundle_id`, `intent_id`, `time_range`, `contents`) — ✅ already done in 5-1
    Evidence:
    - Code: crates/forensic-service/src/bundle.rs
    - Code: crates/forensic-service/src/bundle_contents.rs
    - Schema: infrastructure/migrations/TBD

[ ] Bundle generation: collect intent versions, artifacts, audit events, graph state — ✅ partially done (content collection primitives done; actual collection from services is Phase 4)
    Evidence:
    - Code: forensic-service bundle builder

[ ] Bundle generation API: `POST /forensic-bundles`
    Evidence:
    - Role: forensic-access
    - Code: intent-api forensic endpoint

[ ] Bundle replay capability (replay bundle to reproduce state)
    Evidence:
    - Code: forensic-service replay engine

[ ] Bundle retention policy (configurable per tenant, compliance)
    Evidence:
    - S3 lifecycle policies

[ ] Forensic bundle export: `GET /forensic-bundles/{bundle_id}/download`
    Evidence:
    - Code: intent-api export endpoint
```

---

## 6. Performance Work

**P5-S1 Bounded Slice — Graph traversal benchmark groundwork delivered**
**P5-S2 Bounded Slice — DB query benchmark groundwork delivered**

```
[~] Graph traversal benchmarks (P5-S1 bounded slice — criterion harness delivered; local baseline captured; production optimization gated on P5 full completion)
    Evidence:
    - Code: crates/graph-service/benches/graph_ops.rs (criterion-based, harness=false)
    - Benchmarks: find_reachable (chain-20, chain-50, diamond), find_path (chain-20, diamond, no-route), detect_cycles (chain, with-cycle)
    - Local baseline (April 2026): path_chain_20 ~6.6µs, cycle_detection_with_cycle ~390ns, reachable_chain_unlimited_20 ~4.9µs
    - Tests: cargo test -p graph-service --all-features (78 tests pass)
    - Note: Bounded slice delivers harness infrastructure and local baseline numbers. Actual performance targets (traversal < 50ms for 10k node graph), DB query optimization, and production load testing remain gated on P5 full completion.

[~] DB query benchmarks (P5-S2 bounded slice — criterion harness delivered; in-memory baseline; real PostgreSQL benchmarks gated on P5 full completion)
    Evidence:
    - Code: crates/intent-service/benches/query_latency.rs (criterion-based, harness=false)
    - Benchmarks: intent CRUD (create_tx, get, create_version_with_occ, get_versions_by_intent), approval request queries (list_pending_by_intent, list_pending_by_tenant, update_status), policy snapshot queries (list_by_intent, get_latest, get_by_version)
    - Docs: docs/11-quality/02-evals-and-benchmarks.md (P5-S2 section added)
    - Docs: docs/11-quality/benchmark-baseline-results.md (DB query section added with TBD baseline template)
    - Tests: cargo test -p intent-service --all-features (102 tests pass)
    - Note: Bounded slice delivers harness infrastructure and in-memory baseline numbers. Uses in-memory repositories only — does NOT include actual SQLx/PostgreSQL connection pool overhead. Actual performance targets, production DB connection sizing, and load testing remain gated on P5 full completion.

[ ] Intent diff optimization (caching, parallel computation)
    Evidence:
    - PR merged: <link>
    - Benchmark: diff computation < 100ms for typical intent
    - Code: rebase-engine/diff_cache.rs

[ ] Graph traversal optimization (indexing, query optimization)
    Evidence:
    - PR merged: <link>
    - Benchmark: graph traversal < 50ms for 10k node graph
    - Code: graph-service/indexing.rs

[ ] Database query optimization (indexes, query plans)
    Evidence:
    - PR merged: <link>
    - EXPLAIN ANALYZE on critical queries
    - New indexes: optimization index migration TBD

[ ] Connection pooling (Postgres, NATS)
    Evidence:
    - Code: connection pool configured
    - Benchmark: no connection exhaustion under load

[ ] Load testing: simulate Phase 3 production load
    Evidence:
    - Load test results: 10x normal load sustained
    - No SLO violations under load
    - Report: load-test-results.md
```

---

## 7. Security Hardening

```
[x] Threat model v2 (updated from Phase 1)
    Evidence:
    - Doc: ../14-governance/06-threat-model-v2.md

[~] Penetration testing scope defined (bounded planning artifact — pen test not yet executed)
    Evidence:
    - Doc: ../08-security/06-pen-test-scope.md
    - Scope: API surfaces, graph, approval, audit, console, WebSocket, NATS, cross-tenant boundaries
    - Out of scope: social engineering, physical security, source code review, DoS
    - Note: This is a planning document only. Actual pen testing is Phase 3/4 future work.

[~] Security review for Phase 3 features (bounded — threat model driven)
    Evidence:
    - Doc: ../14-governance/06-threat-model-v2.md (security controls mapping)
    - Doc: ../08-security/05-compliance-checklist.md (control status tracking)
    - Note: Full security review gated on pen test results. Threat model review complete.

[x] Compliance checklist (bounded planning artifact — SOC2/GDPR/ISO27001 control tracking)
    Evidence:
    - Doc: ../08-security/05-compliance-checklist.md
    - Scope: SOC2 CC1-CC8, GDPR Art.5/17/30/32/33/35, ISO27001 A.5/A.6/A.8/A.9/A.10/A.12/A.13/A.16/A.18
    - Note: This is a control-tracking checklist. Certification audit is Phase 4 future work.

[x] Incident response plan documented (bounded planning artifact)
    Evidence:
    - Doc: ../14-governance/14-incident-response-plan.md
    - Scope: SEV1-4, Phases 1-6 (detection through post-incident review), RACI, communication plan
    - Doc: ../14-governance/11-incident-freeze.md (data freeze procedures — already existing)
    - Note: Operational runbooks (RB6-RB9) remain in progress per section 3.

[ ] Data retention and deletion verified
    Evidence:
    - Tests: deletion removes data within SLA
    - S3 lifecycle policies enforced
```

---

## Exit Gate Confirmation

```
ALL ITEMS COMPLETE: □ Yes □ No

Phase 3 Exit Gate Review Date: ___________
Reviewed By: ___________
Product Owner Sign-off: ___________
Security Sign-off: ___________
SRE Sign-off: ___________
Compliance Sign-off: ___________

Blocking Issues (if any):
1.
2.
3.

Notes:
-
```

**Next Phase:** [Phase 4 — Enterprise Expansion](./checklist-phase-4.md)
