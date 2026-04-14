# Phase 3 — Compensation + Production Hardening Checklist

**Exit Gate:** Phase 3 exit gate khi tất cả items checked và có evidence.
**Prerequisite:** Phase 2b exit gate passed. Phase 2b scope includes: runtime adapter external implementation, apply endpoint, risk classification, graph update, replay API, event streaming. Phase 3 Batch 0 (hardening planning and scaffold prep) may proceed in parallel while Phase 2b is in progress — see [05-phase-3-hardening.md](../05-phase-3-hardening.md) for batch structure.

**Trạng thái:** `BATCH 0 COMPLETE, BATCH 1 LARGELY DELIVERED, BATCH 3b BOUNDED VERIFICATION + EXPORT SLICES DELIVERED` — Batch 0 code scaffolds and planning items complete. Batch 1 side effect ledger, compensation action CRUD + APIs, batch orchestration, policy gate, orchestration dashboard, orchestration coordination view, dry-run planner, and single-shot runtime (HTTP + CLI) all delivered. Batch 3b currently includes bounded forensic verification via `POST /forensic/verify` and bounded in-memory export metadata generation via `POST /forensic/export`; bundle generation from real services, storage, download-from-storage, and replay remain future scope. Formal planner/executor/retry/rollback record remains gated on Phase 2b exit. See [05-phase-3-hardening.md](../05-phase-3-hardening.md) for the current execution split.
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

```
[ ] SLO definitions (intent processing latency, rebase latency, approval wait time)
    Evidence:
    - PR merged: <link>
    - Doc: ../../09-operations/04-sre-and-slos.md (updated)
    - Dashboard: Grafana SLO dashboard

[ ] Alerting rules (warning, critical thresholds)
    Evidence:
    - PR merged: <link>
    - Alert rules: alertmanager.yml or equivalent
    - Tests: alerting tests pass (send test alerts)

[ ] Error budget tracking
    Evidence:
    - Dashboard: error budget dashboard
    - Runbook: error budget exceeded response

[ ] Distributed tracing across all services (full Phase 2 → Phase 3 trace)
    Evidence:
    - PR merged: <link>
    - OTel: trace context propagated across all service boundaries
    - Jaeger/Zipkin: trace searchable

[ ] Performance benchmarks: rebase latency p50/p95/p99
    Evidence:
    - Benchmark results: rebase_engine benchmarks
    - Target: p95 < 60s for low/medium risk

[ ] Runbooks for common failure scenarios
    Evidence:
    - Doc: ../../09-operations/05-runbooks.md (updated with Phase 3 scenarios)
    - Runbooks: rebase-stuck, approval-backlog, artifact-quarantine-fail, compensation-timeout
```

---

## 4. Tenant Isolation Hardening

```
[ ] Tenant isolation verification tests
    Evidence:
    - PR merged: <link>
    - Tests: cross-tenant access attempts blocked
    - Tests: data leakage tests (tenant A cannot see tenant B data)

[ ] Resource quota enforcement (intents per tenant, artifacts per tenant)
    Evidence:
    - PR merged: <link>
    - Code: tenant-service/quota.rs
    - Tests: quota enforcement tests pass

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

```
[x] Forensic bundle model (`bundle_id`, `intent_id`, `time_range`, `contents`) — ✅ Batch 0 scaffold
    Evidence:
    - Code: crates/forensic-service/src/bundle.rs
    - Code: crates/forensic-service/src/bundle_contents.rs

[x] Forensic verification API: `POST /forensic/verify` (Phase 3 Batch 3b bounded slice)
    Evidence:
    - Code: crates/forensic-service/src/verification.rs (ForensicVerificationService, types)
    - Code: crates/forensic-service/src/lib.rs (module exports)
    - Code: crates/intent-api/src/lib.rs (handler, DTOs, route wiring)
    - Tests: cargo test -p forensic-service --all-features
    - Tests: cargo test -p intent-api --all-features (forensic verification tests)
    - Doc: docs/04-api/openapi.yaml (Phase 3 Batch 3b section + path definition)
    - Doc: docs/14-governance/10-forensic-bundle.md (bounded slice documentation)
    - Doc: docs/10-delivery/09-completion-proposals-tracker.md (P4 updated)
    - Note: Bounded request-driven verification only. Does NOT claim bundle generation, storage, retrieval, replay, or hash chain integrity.

[ ] Forensic verification integration with real services (Phase 3 Batch 3b+)
    Evidence:
    - Code: forensic-service integration with intent-service, graph-service, audit-service

[ ] Bundle generation: collect intent versions, artifacts, audit events, graph state
    Evidence:
    - S3 layout: forensic-bundles/{tenant}/{bundle_id}/
    - Code: forensic-service bundle builder

[ ] Bundle generation API: `POST /api/v1/forensic/bundle`
    Evidence:
    - Role: forensic-access
    - Code: intent-api forensic endpoint

[ ] Bundle integrity verification (hash chain)
    Evidence:
    - Code: forensic-service integrity verification

[ ] Bundle replay capability (replay bundle to reproduce state)
    Evidence:
    - Code: forensic-service replay engine

[x] Bundle retention policy metadata (configurable per tenant, model-level evidence only — Phase 3 Batch 3b retention-evidence slice)
    Evidence:
    - Code: crates/forensic-service/src/bundle.rs (BundleRetention struct, RetentionPolicy enum, retention field on ForensicBundle)
    - Code: crates/forensic-service/src/bundle.rs (BundleRetention::new, BundleRetention::with_expiry helpers)
    - Tests: cargo test -p forensic-service --all-features (retention metadata tests)
    - Doc: docs/14-governance/10-forensic-bundle.md (retention policy metadata only — truthful scope)
    - Note: **Truthful scope — model-level retention evidence only.** No S3 lifecycle enforcement, no background deletion jobs, no automatic expiry. Retention policy and expiry metadata are recorded on the bundle model. Actual S3 lifecycle rules (GLACIER after 30d, DEEP_ARCHIVE after 3650d) are future phase.

[ ] Bundle retention policy (configurable per tenant, compliance)
    Evidence:
    - S3 lifecycle policies

[x] Forensic archive export API: `POST /forensic/export` (Phase 3 Batch 3b bounded slice)
    Evidence:
    - Code: crates/forensic-service/src/export.rs (ForensicArchiveGenerator, types)
    - Code: crates/forensic-service/src/lib.rs (module exports)
    - Code: crates/intent-api/src/lib.rs (handler, DTOs, route wiring)
    - Tests: cargo test -p forensic-service --all-features (export tests)
    - Tests: cargo test -p intent-api --all-features (export endpoint tests)
    - Doc: docs/04-api/openapi.yaml (forensic export path + schemas)
    - Doc: docs/14-governance/10-forensic-bundle.md (bounded export documentation)
    - Doc: docs/10-delivery/09-completion-proposals-tracker.md (P4 export status)
    - Note: Bounded in-memory archive generation only. Does NOT claim persisted bundles, S3 storage, async jobs, or download-from-storage.

[ ] Forensic bundle export from storage: `GET /api/v1/forensic/bundles/{id}/download`
    Evidence:
    - Code: intent-api stored export/download endpoint
```

---

## 6. Performance Work

```
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
[ ] Threat model v2 (updated from Phase 1)
    Evidence:
    - PR merged: <link>
    - Doc: ../../14-governance/06-threat-model-v2.md

[ ] Penetration testing completed
    Evidence:
    - Report: penetration-test-results.md
    - All critical/high findings remediated

[ ] Security review for all Phase 3 features
    Evidence:
    - Review sign-off: security-team
    - Findings: none critical/high unmitigated

[ ] Compliance checklist (if applicable: SOC2, GDPR, etc.)
    Evidence:
    - Doc: compliance-checklist.md
    - All items checked

[ ] Incident response plan documented
    Evidence:
    - Doc: ../../14-governance/11-incident-freeze.md
    - Runbook: incident-response.md

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
