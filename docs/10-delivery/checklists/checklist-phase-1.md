# Phase 1 — Core Control Plane MVP Checklist

**Exit Gate:** Phase 1 complete khi tất cả items checked và có evidence.  
**Prerequisite:** Phase 0 exit gate passed.

**Trạng thái:** `IN PROGRESS`  
**Phase:** Phase 1  
**Target Duration:** 4–8 tuần

---

## 1. Intent Schema & Versioning

```
[x] Intent data model implemented
    Evidence:
    - Branch: phase1/db-http-wiring (PR pending)
    - Code: crates/intent-rebase-types/src/intent.rs (Intent, IntentVersion, IntentPayload types)
    - Tests: crates/intent-service/src/lib.rs unit tests
    - Docs: docs/03-spec/01-intent-model.md (already present)

[x] Intent versioning (create, update, list versions) implemented
    Evidence:
    - Branch: phase1/db-http-wiring (PR pending)
    - Code: crates/intent-service/src/lib.rs (IntentService.create_intent, create_version, list_versions)
    - Tests: test_create_version, test_list_versions, test_list_versions_descending_order, test_get_specific_version
    - Migration: infrastructure/migrations/002_create_intent_versions.sql

[x] Intent ID generation and uniqueness enforced
    Evidence:
    - Tests: test_in_memory_repo_persistence (verifies in-memory repo persistence with shared state)
    - DB: intents.intent_id is PRIMARY KEY in 001_create_intents.sql

[ ] Intent schema validation (JSON Schema or equivalent)
    Note: IntentPayload deserialization is validated via serde; explicit JSON Schema not yet added
    (Deferred to future PR if Phase 1 validation needed)
```

---

## 2. Semantic Diff v1

```
[x] Diff computation algorithm implemented (engine-core only — PR #5)
    Evidence:
    - PR merged: #5
    - Code: crates/rebase-engine/src/diff.rs
    - Tests: crates/rebase-engine/src/diff.rs and crates/rebase-engine/src/lib.rs

[x] Risk analysis: severity, confidence, manual-review triggers (engine-core only — PR #6)
    Evidence:
    - Branch: phase1/diff-risk-rules-v1 (PR pending)
    - Code: crates/rebase-engine/src/risk.rs, crates/rebase-engine/src/rules.rs
    - Tests: risk.rs, rules.rs, and lib.rs cover severity levels, confidence scoring, manual-review triggers, and engine API behavior

[x] Diff threshold configuration (via rule pack)
    Evidence:
    - Code: crates/rebase-engine/src/rule_pack.rs (RulePack, RulePackVersion, RulePackRiskConfig)
    - Code: crates/rebase-engine/src/rules.rs (analyze_diff_risk_with_config)
    - Tests: fixture_tests module exercises regression fixtures with threshold config
    - Fixture corpus: no-semantic-change.json, scope-add-medium.json loaded and verified
    - DEFAULT_RULE_PACK static provides Phase 1 default thresholds

[x] Diff API endpoint: POST /v1/intents/{id}/diff (PR #7)
    Evidence:
    - Code: crates/intent-api/src/lib.rs (handler and route)
    - Code: crates/intent-service/src/lib.rs (compute_diff method)
    - OpenAPI spec updated: docs/04-api/openapi.yaml
    - REST docs updated: docs/04-api/01-rest-api.md
    - Tests: handler tests and service tests

[x] Structured diff output for scope/constraints/acceptance/authority
    Evidence:
    - Code: crates/rebase-engine/src/diff.rs (IntentVersionDiff and related types)
    - Tests: tests covering section add/remove/modify, determinism, ambiguity fallback, duplicate identity handling
    - NOT including: similarity score (deferred to future PRs)

[x] Engine-local diff risk rules (severity, confidence, manual-review)
    Evidence:
    - Code: crates/rebase-engine/src/risk.rs (DiffRiskAnalysis, Severity, RiskConfig)
    - Code: crates/rebase-engine/src/rules.rs (analyze_diff_risk, matching stats)
    - Tests: tests covering all severity levels, confidence thresholds, manual-review triggers, and engine API behavior
```

---

## 3. Graph Model v1

```
[x] Graph data model (nodes, edges, labels) implemented
    Evidence:
    - PR: PR #9 (merged)
    - Code: crates/intent-rebase-types/src/graph.rs (GraphNode, GraphEdge, NodeType, EdgeType)
    - Code: crates/graph-service/src/lib.rs (GraphService with repository trait)
    - Schema: infrastructure/migrations/004_create_graph_nodes.sql, 005_create_graph_edges.sql

[x] Graph CRUD operations (add node, add edge, query) implemented
    Evidence:
    - PR: PR #9 (merged)
    - Code: crates/graph-service/src/lib.rs (add_node, get_node, list_nodes, add_edge, get_edge, list_edges, list_edges_from, list_edges_to)
    - Tests: crates/graph-service/src/lib.rs tests (test_create_and_get_node, test_create_and_get_edge, test_list_edges_from_and_to, etc.)
    - Repository pattern: InMemoryGraphRepository for Phase 1 baseline

[x] Graph traversal (BFS, path finding) implemented (PR #10)
    Evidence:
    - PR: PR #10 (merged)
    - Code: crates/graph-service/src/lib.rs (find_reachable, find_path, detect_cycles, list_reachable_nodes, are_connected)
    - Code: crates/intent-rebase-types/src/graph.rs (GraphPath, ReachabilityResult, CycleDetectionResult, TraversalOptions)
    - Tests: BFS traversal, path finding, edge filters, cycle detection tests added
    - Scope: internal service layer only (no HTTP endpoints)

[x] Graph ingestors baseline (artifact, approval, side-effect) implemented (PR #11)
    Evidence:
    - PR: PR #11 (stacked dependency / open)
    - Code: crates/intent-rebase-types/src/graph.rs (ArtifactIngestRequest, ApprovalIngestRequest, SideEffectIngestRequest, IngestorResult)
    - Code: crates/graph-service/src/lib.rs (ingest_artifact, ingest_approval, ingest_side_effect methods)
    - Tests: ingestor tests for artifact, approval, and side-effect node creation and edge wiring
    - Scope: internal service layer only (no HTTP endpoints)

[x] Graph classification baseline (PR #12)
    Evidence:
    - PR: PR #12 (merged)
    - Code: crates/intent-rebase-types/src/graph.rs (ClassificationImpact, ClassifiedNode, ClassifyRequest, ClassificationResult)
    - Code: crates/graph-service/src/lib.rs (classify_impact method with deterministic propagation rules)
    - Tests: classification tests for direct/transitive impact, depth bounds, diamond graphs, unreachable nodes
    - Scope: internal service layer only (no HTTP endpoints)
    - Note: Uses deterministic explicit propagation rules, NOT rule-pack-driven propagation

[x] Graph propagation rules from rule pack applied (PR #13) - IMPLEMENTED THIS PR
    Evidence:
    - PR: PR #13 (current PR - rule-pack-driven propagation baseline)
    - Code: crates/intent-rebase-types/src/graph.rs (PropagationConfig, EdgeDirection, DEFAULT_PROPAGATION_CONFIG)
    - Code: crates/rebase-engine/src/rule_pack.rs (RulePackPropagationConfig, RulePack::propagation_config())
    - Code: crates/graph-service/src/lib.rs (classify_impact updated to use PropagationConfig)
    - Tests: backward compat, custom max_depth, custom target_types, empty edge types, approval reachability
    - Scope: internal service layer only (no HTTP endpoints)
    - Note: Propagation config with defaults matching prior hardcoded behavior

[ ] Graph API endpoints: GET /api/v1/graph, POST /api/v1/graph/nodes, POST /api/v1/graph/edges
    Note: Deferred to future PR - internal CRUD only in Phase 1 baseline
    Evidence:
    - Code: not yet implemented (no public HTTP endpoints per PR #9 scope)
```

---

## 4. Rebase Preview Only

```
[x] Rebase plan computation implemented (PR #14 - planner baseline)
    Evidence:
    - PR: PR #14 (merged)
    - Code: crates/rebase-engine/src/planner.rs (DecisionClass, RebasePlan, section decisions)
    - Code: crates/rebase-engine/src/lib.rs (RebaseEngine::generate_plan, generate_plan_with_risk)
    - Tests: crates/rebase-engine/src/planner.rs unit tests (14 tests covering A/B/C/D/E mapping)
    - Tests: deterministic ordering and risk level mapping tests

[x] Rebase preview includes decision class, rationale, section decisions, risk level
    Evidence:
    - Code: RebasePreviewResponse in crates/intent-api/src/lib.rs (intent_id, from_version, to_version, decision_class, rationale, section_decisions, manual_review_recommended, risk_level)
    - OpenAPI: RebasePreviewResponse schema in docs/04-api/openapi.yaml
    - Tests: test_rebase_preview_success verifies rationale and risk_level fields
    - Note: AffectedItemsPreview is graph-integrated in Phase 1 PR #16 (not Phase 2)

[x] NO rebase apply in Phase 1 — preview only
    Evidence:
    - Deferred fields remain internal typed groundwork only (not exposed in preview response)
    - No apply method exists in RebaseEngine (Phase 2)
    - generate_plan returns typed RebasePlan for preview only

[x] Rebase preview endpoint: POST /v1/intents/{id}/rebase-preview
    Evidence:
    - Code: crates/intent-api/src/lib.rs (rebase_preview handler and route)
    - OpenAPI: docs/04-api/openapi.yaml (RebasePreviewResponse schema, computeRebasePreview operation)
    - Tests: test_rebase_preview_success, test_rebase_preview_invalid_version_ordering
    - Phase 1 returns: decision_class, rationale, section_decisions, affected_items (graph-integrated), manual_review_recommended, risk_level
    - Phase 2: deferred fields remain internal groundwork only

[x] Graph-integrated affected items (PR #16)
    Evidence:
    - PR: PR #16 (current PR - graph-integrated affected items)
    - Code: crates/graph-service/src/lib.rs (classify_impact with ValidatedBy support)
    - Code: crates/intent-rebase-types/src/graph.rs (DEFAULT_PROPAGATION_CONFIG includes ValidatedBy)
    - Code: crates/intent-service/src/lib.rs (compute_rebase_preview_with_graph wiring)
    - Code: crates/intent-api/src/lib.rs (graph-integrated rebase_preview handler)
    - Tests: test_classify_approval_via_validated_by (Validates ValidatedBy traversal)
    - Tests: test_rebase_preview_success (API handler with graph integration)
    - Affected item types: artifacts (DependsOn), approvals (ValidatedBy), side_effects (Triggers/GeneratedFrom)
    - Status tracking: AffectedItemsStatus::Available/Unavailable honestly communicates graph data availability

[x] Apply/checkpoint typed contracts groundwork (PR #17)
    Evidence:
    - PR: PR #17 (current PR - typed apply/checkpoint contracts)
    - Code: crates/rebase-engine/src/planner.rs (CheckpointSelection, ApprovalRevalidation, CompensationReadiness and variants)
    - Code: crates/rebase-engine/src/lib.rs (exports for new types)
    - Tests: test_deferred_fields_typed_groundwork, test_checkpoint_selection_deferred, test_approval_revalidation_deferred, test_compensation_readiness_deferred
    - All Phase 2 fields have ready=false until Phase 2 runtime execution exists

[x] Internal checkpoint selection heuristic baseline (PR #18)
    Evidence:
    - PR: PR #18 (internal checkpoint heuristic baseline only)
    - Code: crates/rebase-engine/src/planner.rs (`CheckpointSelection::heuristic_baseline`, `compute_checkpoint_candidates`, `select_best_checkpoint`)
    - Tests: test_checkpoint_selection_heuristic_class_c_prefers_nearest_validated, test_checkpoint_selection_heuristic_class_d_prefers_pre_side_effect, test_checkpoint_selection_heuristic_class_e_requires_manual_handoff
    - `CheckpointSelection.ready` remains false; candidates and selected values are internal hints only
    - Apply HTTP endpoint still deferred

[x] Internal approval revalidation heuristic baseline (PR #19)
    Evidence:
    - PR: PR #19 (internal approval revalidation heuristic baseline only)
    - Code: crates/rebase-engine/src/planner.rs (`ApprovalRevalidation::heuristic_baseline`, `compute_approvals_needing_revalidation`, `select_revalidation_strategy`, `build_revalidation_rationale`)
    - Code: crates/intent-service/src/lib.rs (compute_rebase_preview_with_graph rebuilds deferred with graph-derived affected_items)
    - Tests: test_approval_revalidation_heuristic_class_a/b/c_incremental_with_graph_approvals/c_incremental_empty_fallback/c_d_full_with_graph_approvals/c_d_full_empty_fallback/c_e_drop/maps_graph_affected_approvals_correctly, test_deferred_fields_uses_heuristic_baseline_not_deferred
    - Uses graph-derived `affected_approvals` when available (Class C/D); falls back to empty with truthful rationale when unavailable
    - Class E drops all approvals regardless of graph data (clean slate before manual handoff)
    - `ApprovalRevalidation.ready` remains false; execution deferred to Phase 2 runtime adapter
    - No public API or OpenAPI changes; internal-only heuristic baseline
```

---

## 5. Console Basic (Frontend MVP)

```
[ ] Intent list view (list all intents, filterable)
    Evidence:
    - PR merged: <link>
    - UI: Next.js page /intents

[ ] Intent detail view (version history, diff viewer)
    Evidence:
    - PR merged: <link>
    - UI: Next.js page /intents/[id]

[ ] Rebase preview display (affected artifacts, approvals, risk)
    Evidence:
    - PR merged: <link>
    - UI: Next.js component RebasePreview

[ ] Basic authentication (login/logout, session management)
    Evidence:
    - PR merged: <link>
    - Auth: nextauth.js or equivalent

[ ] Tenant context switching (for multi-tenant dev testing)
    Evidence:
    - Dev mode: tenant selector in UI
    - Production: tenant extracted from auth token
```

---

## 6. Audit Baseline

```
[ ] Audit event model defined (event_type, actor, target, timestamp, metadata)
    Evidence:
    - Doc: ../../14-governance/01-audit-event-spec.md
    - Code: audit-service/event.rs

[ ] Audit event append API implemented
    Evidence:
    - PR merged: <link>
    - Code: audit-service/append.rs
    - Tests: audit-service/tests/append_tests.rs

[ ] Audit event query API implemented (filter by tenant, time range, event type)
    Evidence:
    - PR merged: <link>
    - Code: audit-service/query.rs
    - Tests: query tests pass

[ ] Audit events persisted to PostgreSQL
    Evidence:
    - Migration: 006_audit_events.sql
    - No UPDATE/DELETE on audit table (append-only)

[ ] Audit event streaming to NATS (if ADR-04 uses NATS)
    Evidence:
    - Code: audit-service/stream.rs
    - Integration test: events appear in NATS subject
```

---

## 7. Data Schema & Migrations

```
[x] Phase 1 schemas migrated in order (001-003)
    Evidence:
    - Migration files: infrastructure/migrations/001_create_intents.sql,
      002_create_intent_versions.sql, 003_create_intent_clauses.sql
    - Migrations use IF NOT EXISTS and are idempotent (CREATE TABLE IF NOT EXISTS)
    - Test: unit tests pass with in-memory repo; SQL repo tests require live DB

[ ] Post-migration data validation tests
    Note: Integration tests with live DB not yet added to CI (PR #4 SQL repo is implemented
    but DB integration tests are skipped in CI; can be added in future PR)
    Evidence:
    - SQL repo implementation: crates/intent-service/src/sqlx_repository.rs
    - Unit tests: crates/intent-service/src/sqlx_repository.rs tests (deserialization,
      helper functions — no live DB required)
    - Note: Live DB integration tests to be added in follow-up PR

[ ] Rollback plan documented for each migration
    Note: Rollback steps not explicitly documented in migration files
    Evidence:
    - Migration comments document forward semantics only
```

---

## 8. Observability v1

```
[ ] Structured JSON logging implemented in all services
    Evidence:
    - PR merged: <link>
    - Code: all services use tracing/log structured

[ ] Prometheus metrics exposed on /metrics
    Evidence:
    - All HTTP services expose /metrics
    - Metrics: intent_operations_total, rebase_previews_total, graph_operations_total

[ ] Health check endpoints: /health, /ready
    Evidence:
    - All services expose /health
    - Kubernetes readiness probe configured

[ ] OTel tracing (basic span instrumentation)
    Evidence:
    - PR merged: <link>
    - Spans: intent.create, intent.update, diff.compute, graph.traverse

[ ] Loki or equivalent log aggregation configured
    Evidence:
    - Dev: loki container in docker-compose
    - Prod: cloud logging configured
```

---

## 9. Security v1

```
[ ] API authentication: API key + JWT validation
    Evidence:
    - PR merged: <link>
    - Middleware: auth.rs
    - Tests: auth tests pass

[ ] Tenant isolation: tenant_id extracted from token, not request
    Evidence:
    - Tests: cross-tenant access blocked
    - Doc: ../../08-security/02-authn-authz.md (updated)

[ ] Input validation on all API endpoints
    Evidence:
    - Code: all endpoints validate input
    - Tests: invalid input rejected

[ ] No PII in logs (tenant_id only, no user email/name)
    Evidence:
    - Log review: no PII present
    - Doc: ../../08-security/03-privacy-and-data-handling.md (updated)
```

---

## 10. API Contract & Documentation

```
[x] OpenAPI 3.0 spec for Phase 1 intent/version endpoints
    Evidence:
    - File: docs/04-api/openapi.yaml (Phase 1 endpoints: createIntent, getIntentHead,
      createVersion, listVersions, getVersion — with OCC headers documented)
    - Routes manually wired in crates/intent-api/src/lib.rs

[ ] Event contracts documented
    Note: NATS subjects not yet defined (deferred to Phase 2+)
    Evidence:
    - Doc: docs/04-api/02-events.md (skeleton only)

[ ] Webhook payload schemas documented
    Note: Webhooks not yet implemented (deferred to Phase 2+)
    Evidence:
    - Doc: docs/04-api/03-webhooks.md (skeleton only)

[x] API change policy: OpenAPI spec must update with code
    Note: No CI enforcement yet; policy documented in agent guide
    Evidence:
    - Doc: docs/12-agents/01-agent-implementation-guide.md (existing)
```

---

## Exit Gate Confirmation

```
ALL ITEMS COMPLETE: □ Yes □ No

Phase 1 Exit Gate Review Date: ___________
Reviewed By: ___________
Product Owner Sign-off: ___________
Security Sign-off: ___________

Blocking Issues (if any):
1.
2.
3.

Notes:
-
```

**Next Phase:** [Phase 2 — Runtime-Integrated Rebase](./checklist-phase-2.md)
