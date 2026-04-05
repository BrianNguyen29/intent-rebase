# Intent Rebase Engine

Phase 1 first slice implementation — Intent Registry. A control layer that manages intent versioning, semantic diff, dependency graphs, and rebase operations for agent runtimes.

## Workspace Structure

```
├── Cargo.toml                 # Root workspace manifest
├── rust-toolchain.toml        # Rust stable toolchain (1.75+)
│
├── crates/
│   ├── intent-rebase-types/   # Shared domain types, traits, errors
│   ├── intent-service/        # Intent CRUD and versioning service
│   ├── intent-api/            # HTTP transport layer (axum)
│   ├── rebase-engine/         # Semantic diff and rebase planning
│   └── graph-service/         # Dependency graph management
│
├── infrastructure/
│   ├── local/
│   │   └── docker-compose.yml # Local dev environment
│   └── migrations/            # SQL migration files
│
└── docs/
    └── 04-api/
        └── openapi.yaml       # OpenAPI 3.0 specification
```

## Crates

| Crate | Purpose | Phase | Status |
|-------|---------|-------|--------|
| `intent-rebase-types` | Shared types: Intent, Artifact, GraphNode, AuditEvent, error types | P0 | ✅ Complete |
| `intent-service` | Intent lifecycle (create, read, update, version) | P1 | ✅ Complete |
| `intent-api` | HTTP transport layer with axum | P1 | ✅ Complete |
| `rebase-engine` | Structured diff core with risk analysis; rule-pack versioning and regression fixtures added; preview-only rebase planner baseline (PR #14); graph-integrated affected items (PR #16); apply/checkpoint typed contracts groundwork (PR #17) | P1 | ✅ Complete (diff, risk, planner baseline; PR #16 graph integration; PR #17 typed apply/checkpoint contracts; apply and checkpoint execution deferred to Phase 2) |
| `graph-service` | Dependency graph CRUD, traversal primitives (BFS, path-finding, cycle detection); ingestors baseline for artifact, approval, and side-effect nodes; classification baseline with deterministic propagation rules and rule-pack propagation baseline | P1 | 🟡 Partial (traversal, ingestors, classification, and rule-pack propagation baseline done; HTTP API and full S3-based rule pack registry deferred) |

## Implementation Status

### Phase 1 First Slice — Intent Registry (Current)
**Implemented:**
- Intent domain types matching `docs/03-spec/01-intent-model.md`
- Intent service with create, version management
- In-memory repository for testing
- SQL-backed repository (`SqlxIntentRepository`) with PostgreSQL/sqlx
- Optimistic concurrency control (OCC) for version creation via `create_version_with_occ`
- HTTP transport layer using axum with manual route binding (CORS only — no tracing middleware)
- Migration files for `intents`, `intent_versions`, `intent_clauses` tables
- OpenAPI 3.0 spec at `docs/04-api/openapi.yaml` (manually wired to handlers)
- Routes mount directly; intended to be served under `/v1` prefix in production
- Rebase preview with graph-integrated affected items (PR #16)

**Deferred to Phase 2+:**
- Full rebase apply operations
- Full authentication/authorization
- DB integration tests in CI (SQL repository is implemented but live-DB tests are skipped)
- Checkpoint selection and runtime adapter integration

### Phase 1 Second Slice — Structured Diff Core (PR #5)
**Implemented:**
- `rebase-engine` structured diff for 4 sections: scope, constraints, acceptance_criteria, authority
- Deterministic output ordering (sorted by clause_id/section)
- Conservative matching rules: prefer clause_id matching; fallback to add/remove when identity ambiguous
- Typed diff output via `IntentVersionDiff` and related types
- Synchronous (`compute_diff_sync`) and async (`RebaseEngine::compute_diff`) APIs

### Phase 1 Third Slice — Diff Risk Rules (PR #6)
**Implemented:**
- Engine-local severity assignment (low/medium/high/critical) based on change type and affected section
- Deterministic confidence scoring (0.0-1.0) based on clause_id matching quality
- Manual-review triggers for critical severity, low confidence, policy changes, multiple high-severity changes
- Typed risk output via `DiffRiskAnalysis`, `Severity`, `RiskConfig` types
- APIs: `analyze_diff_risk()`, `compute_diff_with_risk_sync()`, `RebaseEngine::compute_diff_with_risk()`
- Tests covering all severity levels, confidence thresholds, and manual-review trigger conditions

**Implemented in this slice (PR #7):**
- Diff API HTTP endpoint: `POST /v1/intents/{intent_id}/diff` with full risk analysis

**NOT implemented in this slice (deferred to future PRs):**
- Graph model and rebase planner integration

**Notes:**
- `intent-api` crate exposes axum router; `build_router()` returns Router that mounts directly
- No tracing middleware (only CORS layer enabled in this PR)
- OCC headers (`X-Expected-Version`, `X-Expected-Row-Version`) are validated at the API boundary:
  malformed headers (non-integer values) return 400 Bad Request instead of being silently ignored
- SQL deserialization returns 500 SerializationError on data corruption; no silent payload fabrication

### Phase 1 Fourth Slice — Diff Governance Hardening (PR #8)
**Implemented:**
- Rule pack versioning support via `RulePack`, `RulePackVersion`, `RulePackRiskConfig` types
- File-based rule pack configuration (JSON serialization/deserialization)
- `RulePack::from_file()` for loading rule packs from filesystem
- `DEFAULT_RULE_PACK` static for Phase 1 default configuration
- `analyze_diff_risk_with_config()` API for configurable threshold analysis
- `analyze_diff_risk()` continues to use default configuration for backward compatibility
- Regression fixtures in `crates/rebase-engine/fixtures/`:
  - `no-semantic-change.json`: Identical content, Low severity, High confidence
  - `scope-add-medium.json`: Scope item added, Medium severity
- Fixture integration tests validating deterministic behavior
- `RulePackRiskConfig` conversion to/from `RiskConfig` for engine integration

**Engine API additions:**
- `RulePack` and related types exported from `rebase_engine` crate
- `analyze_diff_risk_with_config()` exported for custom threshold analysis
- Full conversion between `RulePackRiskConfig` and `RiskConfig`

**NOT in scope for this slice (deferred to future PRs):**
- S3-based rule pack storage (ADR-06 full implementation)
- Multi-tenant pack customization
- Rule pack registry database table

### Phase 1 Fifth Slice — Graph Traversal Baseline (PR #10)
**Implemented:**
- BFS reachability queries via `find_reachable()` and `list_reachable_nodes()`
- Shortest path finding via `find_path()`
- Cycle detection via `detect_cycles()`
- Connectivity checking via `are_connected()`
- Edge type filtering in traversal options
- Max depth limiting for bounded traversal
- Types: `GraphPath`, `ReachabilityResult`, `CycleDetectionResult`, `TraversalOptions`

**NOT in scope for this slice (deferred to future PRs):**
- Graph propagation rules from rule packs
- Graph API HTTP endpoints
- Impact classification output semantics

### Phase 1 Sixth Slice — Graph Ingestors Baseline (PR #11)
**Implemented:**
- `ArtifactIngestRequest`, `ApprovalIngestRequest`, `SideEffectIngestRequest` types for structured ingestion
- `ingest_artifact()` method: creates Artifact node with DependsOn edges to IntentVersion nodes
- `ingest_approval()` method: creates Approval node with GovernedBy edge to PolicySnapshot and ValidatedBy edge to IntentVersion
- `ingest_side_effect()` method: creates SideEffect node with Triggers edge from TaskNode, DerivedFrom edge to IntentVersion, and GeneratedFrom edge to Approval
- `IngestorResult` type wrapping created node and edges
- Unit tests for deterministic ingestion behavior and edge wiring

**NOT in scope for this slice (deferred to future PRs):**
- Graph propagation rules from rule packs
- Graph classification output semantics
- External runtime adapters

### Phase 1 Seventh Slice — Graph Classification Baseline (PR #12)
**Implemented:**
- `ClassificationImpact` enum: Direct, Transitive, Unchanged
- `ClassifiedNode` type: classified node with impact level and human-readable reason
- `ClassifyRequest` and `ClassificationResult` types for classification input/output
- `classify_impact()` method: baseline classification using deterministic explicit propagation rules
- Edge direction semantics: DependsOn (incoming), Triggers (outgoing), GeneratedFrom (outgoing)
- Bounded depth traversal (default max_depth=3) for controlled propagation
- Unit tests: direct/transitive impact, depth bounds, diamond graphs, unreachable nodes, empty graph

**Classification propagation rules (baseline):**
- Direct: Nodes at depth 1 from start (e.g., Artifacts directly depending on IntentVersion)
- Transitive: Nodes at depth 2+ (e.g., SideEffects triggered downstream)
- Bounded depth prevents runaway traversal

**NOT in scope for this slice (deferred to future PRs):**
- Rule-pack-driven propagation rules
- Graph HTTP API endpoints
- Rebase planner integration
- Full rule-pack integration

### Phase 1 Eighth Slice — Rule-Pack Propagation Baseline (PR #13)
**Implemented:**
- `PropagationConfig` type in `intent-rebase-types` for propagation configuration (max_depth, traversable_edge_types, traversable_directions, target_node_types)
- `EdgeDirection` enum: Incoming, Outgoing, Both
- `RulePackPropagationConfig` in `rebase-engine` for rule-pack-driven propagation
- `DEFAULT_PROPAGATION_CONFIG` static for Phase 1 default propagation behavior
- `RulePack::propagation_config()` method to get `PropagationConfig` from a rule pack
- `ClassifyRequest.propagation_config` optional field for custom propagation behavior
- `classify_impact()` updated to use `PropagationConfig` when provided, with backward compatibility for existing callers
- `enqueue_propagation_edges_with_config()` helper method respecting propagation config
- Unit tests: backward compat with None config, custom max_depth override, custom target_types, empty edge types, approval reachability

**Propagation configuration defaults (matching Phase 1 baseline):**
- max_depth: 3
- traversable_edge_types: DependsOn, Triggers, GeneratedFrom
- target_node_types: Artifact, Approval, SideEffect, Generic
- Direction semantics unchanged: DependsOn (incoming), Triggers/GeneratedFrom (outgoing)

**NOT in scope for this slice (deferred to future PRs):**
- S3 rule pack registry/loader
- Full rule-pack-driven propagation with custom rule packs
- Graph HTTP API endpoints
- Rebase planner integration

### Phase 1 Ninth Slice — Rebase Planner Baseline (PR #14)
**Implemented:**
- `DecisionClass` enum: A (No-op), B (Soft review), C (Partial repair), D (Compensation + repair), E (Hard restart)
- `RebasePlan` type with decision class, rationale, section decisions, and risk level
- `AffectedItemsPreview` and `DeferredFields` for Phase 2 extensibility (TODO markers)
- Deterministic decision class mapping from diff+risk analysis
- `RebaseEngine::generate_plan()` async method for typed rebase plan generation
- `RebaseEngine::generate_plan_with_risk()` for pre-computed risk analysis
- Unit tests covering all decision classes (A-E) and deterministic ordering

**Decision class mapping (Phase 1 baseline):**
- Class A: No semantic changes (empty diff)
- Class B: Low/Medium severity changes requiring soft review
- Class C: High severity changes in limited scope (auto-repair candidate); also Medium + 2 changed sections
- Class D: High severity with manual review or 2+ high-severity sections; also Medium + manual review OR 3+ changed sections
- Class E: Critical severity OR 3+ high-severity sections (manual handoff required)

**NOT in scope for this slice (deferred to Phase 2):**
- Checkpoint selection heuristics
- Approval revalidation hooks
- Runtime adapter integration
- Rebase apply HTTP endpoint

### Phase 1 Tenth Slice — Graph-Integrated Affected Items (PR #16)
**Implemented:**
- `AffectedItemsPreview` updated with `status` field (available/unavailable) for truthful data availability reporting
- `AffectedItem` type: node_id, label, impact, reason, external_ref
- `AffectedItemsStatus` enum: available, unavailable
- `find_intent_version_node()` helper in graph-service: locate IntentVersion node by intent_version_id
- `classify_affected_items_from_intent_version()` convenience method combining lookup and classification
- `IntentService::with_graph_service()` constructor accepting GraphService
- `IntentService::compute_rebase_preview_with_graph()` enriching rebase preview with graph classification
- Graph integration in rebase-preview endpoint: endpoint remains functional when graph unavailable (status=unavailable)
- Service wiring: IntentService is the single graph-service owner for preview enrichment
- OpenAPI spec updated: AffectedItemsPreview, AffectedItemsStatus, AffectedItem, ClassificationImpact schemas

**Classification behavior:**
- Starts from target IntentVersion graph node (to_version)
- Classifies reachable Artifact, Approval, SideEffect nodes within max_depth=3
- Direct: depth 1, Transitive: depth 2+
- Returns `status: unavailable` gracefully when graph node not found or service unavailable

**Reliability guarantee:**
- The rebase-preview endpoint does NOT fail with 500/404 when graph coverage is incomplete
- `status: unavailable` honestly signals incomplete data without pretending arrays are authoritative
- Compensation actions are NOT generated in this slice (Phase 2 feature)

**NOT in scope for this slice (deferred to Phase 2):**
- Runtime-backed checkpoint selection
- Approval revalidation hooks
- Runtime adapter integration
- Rebase apply (preview-only)
- `deferred` fields in preview response

### Phase 1 Eleventh Slice — Apply/Checkpoint Groundwork (PR #17)
**Implemented:**
- Typed internal contracts replacing stringly TODO placeholders in `DeferredFields`:
  - `CheckpointSelection`: ready flag, candidates vector, selected option, rationale option
  - `CheckpointCandidate`: id, label, description, validated flag
  - `ApprovalRevalidation`: ready flag, approvals_needing_revalidation vector, strategy enum, rationale option
  - `ApprovalNeedingRevalidation`: node_id, label, original_rule_id, reason
  - `RevalidationStrategy`: Deferred, Full, Incremental, Drop variants
  - `CompensationReadiness`: ready flag, potential_actions vector, has_irreversible_effects flag, rationale option
  - `CompensationAction`: id, label, description, reversible flag, priority
- PR #17 established `ready: false` groundwork and empty defaults before later internal heuristics
- Deterministic unit tests verify deferred state properties for all new types
- Apply HTTP endpoint NOT added (deferred to Phase 2)

**New types exported from `rebase_engine` crate:**
- `CheckpointSelection`, `CheckpointCandidate`
- `ApprovalRevalidation`, `ApprovalNeedingRevalidation`, `RevalidationStrategy`
- `CompensationReadiness`, `CompensationAction`

**NOT in scope for this slice (deferred to Phase 2):**
- Runtime-backed checkpoint discovery/execution
- Approval revalidation execution hooks
- Runtime adapter integration
- Rebase apply HTTP endpoint

### Phase 1 Twelfth Slice — Internal Checkpoint Heuristic Baseline (PR #18)
**Implemented:**
- Internal `CheckpointSelection::heuristic_baseline()` for deterministic checkpoint strategy hints by decision class
- Class C prefers the nearest validated checkpoint before the first invalidated node
- Class D prefers a checkpoint before irreversible side effects when possible
- Class E surfaces a manual-handoff boundary without auto-selecting a restart point
- `CheckpointSelection.ready` remains `false` because runtime-backed checkpoint execution is still deferred

**NOT in scope for this slice (deferred to Phase 2):**
- Runtime-backed checkpoint lookup or replay
- Approval revalidation execution hooks
- Runtime adapter integration
- Rebase apply HTTP endpoint

### Phase 1 Thirteenth Slice — Internal Approval-Revalidation Heuristic Baseline (PR #19)
**Implemented:**
- Internal `ApprovalRevalidation::heuristic_baseline()` for deterministic strategy hints by decision class
- Uses graph-derived `affected_approvals` when available (via PR #16 graph integration); falls back to empty when unavailable
- Class C (incremental): maps graph approvals to `ApprovalNeedingRevalidation` when present; falls back to empty with truthful rationale
- Class D (full): maps graph approvals when present; falls back to empty when unavailable
- Class E (drop): discards all approvals regardless of graph data (clean slate before manual handoff)
- Class A/B: no approvals need revalidation; rationale explains no immediate action needed
- `ApprovalRevalidation.ready` remains `false` because runtime-backed execution is still deferred
- `compute_rebase_preview_with_graph` in intent-service rebuilds `DeferredFields` with graph-derived affected items so the heuristic can use them
- `AffectedItemsPreview::unavailable()` is passed in `RebasePlan::from_diff_and_risk` (graph integration happens upstream in the service layer)

**NOT in scope for this slice (deferred to Phase 2):**
- Approval revalidation runtime execution hooks
- Runtime adapter integration
- Rebase apply HTTP endpoint

### Phase 2+ — Rebase, Graph, Extended Diff
- Rebase planner (graph-based)
- Impact graph propagation
- Runtime adapter integration

## Quick Start

```bash
# Start local infra
docker compose -f infrastructure/local/docker-compose.yml up -d

# Check workspace compiles
cargo check --workspace

# Run tests
cargo test --workspace

# Format check
cargo fmt --all -- --check

# Apply database migrations (when ready)
# psql $DATABASE_URL -f infrastructure/migrations/001_create_intents.sql
# psql $DATABASE_URL -f infrastructure/migrations/002_create_intent_versions.sql
# psql $DATABASE_URL -f infrastructure/migrations/003_create_intent_clauses.sql
```

## API Endpoints (Phase 1 Implemented)

Routes are mounted directly (e.g., `POST /intents`) and intended to be served under `/v1` prefix in production deployments.

| Method | Endpoint | Description |
|--------|----------|-------------|
| POST | `/v1/intents` | Create a new intent |
| GET | `/v1/intents/{intent_id}` | Get intent head (current version) |
| POST | `/v1/intents/{intent_id}/versions` | Create a new version (supports OCC via `X-Expected-Version` / `X-Expected-Row-Version` headers) |
| GET | `/v1/intents/{intent_id}/versions` | List all versions |
| GET | `/v1/intents/{intent_id}/versions/{version_number}` | Get specific version |
| POST | `/v1/intents/{intent_id}/diff` | Compute semantic diff between two versions (Phase 1 Diff Preview) |
| POST | `/v1/intents/{intent_id}/rebase-preview` | Compute preview rebase plan with graph-integrated affected items when available (PR #16) |

Full OpenAPI spec: `docs/04-api/openapi.yaml`

## ADRs

All ADRs are in `docs/13-adrs/` and reflect accepted/deferred status as of Phase 0.

## Local Environment Variables

Copy `.env.example` to `.env` and fill in values. Required variables:

```env
DATABASE_URL=postgres://intent_rebase:intent_rebase_dev@localhost:5432/intent_rebase
NATS_URL=nats://localhost:4222
AWS_ACCESS_KEY_ID=minioadmin
AWS_SECRET_ACCESS_KEY=minioadmin
S3_ENDPOINT=http://localhost:9000
S3_REGION=us-east-1
S3_BUCKET=intent-rebase-artifacts
```
