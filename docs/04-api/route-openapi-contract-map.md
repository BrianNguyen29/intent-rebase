# Route / OpenAPI Contract Map

> **Purpose:** Documented contract map between router paths and OpenAPI spec paths.
> Used for drift detection during code review and as a validation source for tests.

## How to Validate This Map

1. **Automated:** `cargo test -p intent-api --lib router_smoke_tests` verifies routes are wired and reachable.
2. **Manual:** Cross-reference each row against `docs/04-api/openapi.yaml`.
3. **On Change:** When adding or removing a route, update this map and the OpenAPI spec in the same PR.

## Path Mapping

| Router Path | OpenAPI Path | Method | Tag | Handler Module | Status |
|-------------|--------------|--------|-----|----------------|--------|
| `/health` | `/health` | GET | health | `health_routes` | ✅ Implemented |
| `/ready` | `/ready` | GET | health | `health_routes` | ✅ Implemented |
| `/metrics` | `/metrics` | GET | health | `health_routes` | ✅ Implemented |
| `/v1/intents/validate` | `/v1/intents/validate` | POST | intents | `intent_validation_handlers` | ✅ Implemented |
| `/intents` | `/intents` | POST | intents | `intent_mutation_handlers` | ✅ Implemented |
| `/intents/:intent_id` | `/intents/{intent_id}` | GET | intents | `intent_read_handlers` | ✅ Implemented |
| `/intents/:intent_id/versions` | `/intents/{intent_id}/versions` | POST | versions | `intent_mutation_handlers` | ✅ Implemented |
| `/intents/:intent_id/versions` | `/intents/{intent_id}/versions` | GET | versions | `intent_read_handlers` | ✅ Implemented |
| `/intents/:intent_id/versions/:version_number` | `/intents/{intent_id}/versions/{version_number}` | GET | versions | `intent_read_handlers` | ✅ Implemented |
| `/intents/:intent_id/diff` | `/intents/{intent_id}/diff` | POST | diff | `diff_handlers` | ✅ Implemented |
| `/intents/:intent_id/rebase-preview` | `/intents/{intent_id}/rebase-preview` | POST | rebase | `rebase_preview_handlers` | ✅ Implemented |
| `/intents/:intent_id/rebase-apply` | `/intents/{intent_id}/rebase-apply` | POST | rebase | `rebase_apply_handlers` | ✅ Implemented |
| `/intents/:intent_id/replay` | `/intents/{intent_id}/replay` | POST | rebase | `replay_handlers` | ✅ Implemented |
| `/intents/:intent_id/side-effects` | `/intents/{intent_id}/side-effects` | GET | side-effects | `query_handlers` | ✅ Implemented |
| `/intents/:intent_id/rebase-simulation` | `/intents/{intent_id}/rebase-simulation` | GET | rebase | `simulation_handlers` | ✅ Implemented |
| `/compensation-simulation/run` | `/compensation-simulation/run` | POST | compensation | `simulation_handlers` | ✅ Implemented |
| `/intents/:intent_id/orchestration-dashboard` | `/intents/{intent_id}/orchestration-dashboard` | GET | compensation | `query_handlers` | ✅ Implemented |
| `/intents/:intent_id/impact-report` | `/intents/{intent_id}/impact-report` | GET | impact-report | `query_handlers` | ✅ Implemented |
| `/intents/:intent_id/compensation-actions` | `/intents/{intent_id}/compensation-actions` | GET | compensation | `compensation_query_handlers` | ✅ Implemented |
| `/compensation-actions/:action_id/approve` | `/compensation-actions/{action_id}/approve` | POST | compensation | `compensation_mutation_handlers` | ✅ Implemented |
| `/compensation-actions/:action_id/waive` | `/compensation-actions/{action_id}/waive` | POST | compensation | `compensation_mutation_handlers` | ✅ Implemented |
| `/compensation-actions/:action_id/execute` | `/compensation-actions/{action_id}/execute` | POST | compensation | `compensation_mutation_handlers` | ✅ Implemented |
| `/compensation-actions/:action_id/reapprove` | `/compensation-actions/{action_id}/reapprove` | POST | compensation | `compensation_mutation_handlers` | ✅ Implemented |
| `/compensation-actions/plan` | `/compensation-actions/plan` | POST | compensation | `compensation_planner_handlers` | ✅ Implemented |
| `/compensation-actions/dlq` | `/compensation-actions/dlq` | GET | compensation | `compensation_query_handlers` | ✅ Implemented |
| `/compensation-actions/batch-candidates` | `/compensation-actions/batch-candidates` | GET | compensation | `compensation_query_handlers` | ✅ Implemented |
| `/compensation-actions/policy-gate` | `/compensation-actions/policy-gate` | GET | compensation | `compensation_query_handlers` | ✅ Implemented |
| `/intents/:intent_id/compensation-policy-gate` | `/intents/{intent_id}/compensation-policy-gate` | GET | compensation | `compensation_query_handlers` | ✅ Implemented |
| `/compensation-actions/orchestration-coordination` | `/compensation-actions/orchestration-coordination` | GET | compensation | `compensation_query_handlers` | ✅ Implemented |
| `/intents/:intent_id/orchestration-coordination` | `/intents/{intent_id}/orchestration-coordination` | GET | compensation | `compensation_query_handlers` | ✅ Implemented |
| `/compensation-actions/orchestration-dry-run` | `/compensation-actions/orchestration-dry-run` | POST | compensation | `compensation_planner_handlers` | ✅ Implemented |
| `/compensation-actions/batch-approve` | `/compensation-actions/batch-approve` | POST | compensation | `batch_handlers` | ✅ Implemented |
| `/compensation-actions/batch-reapprove` | `/compensation-actions/batch-reapprove` | POST | compensation | `batch_handlers` | ✅ Implemented |
| `/compensation-actions/batch-execute` | `/compensation-actions/batch-execute` | POST | compensation | `batch_handlers` | ✅ Implemented |
| `/compensation-actions/runs` | `/compensation-actions/runs` | POST | compensation | `orchestration_run_handlers` | ✅ Implemented |
| `/compensation-actions/runs/:run_id` | `/compensation-actions/runs/{run_id}` | GET | compensation | `orchestration_run_handlers` | ✅ Implemented |
| `/v1/graph/nodes` | `/v1/graph/nodes` | POST | graph | `graph_handlers` | ✅ Implemented |
| `/v1/graph/nodes` | `/v1/graph/nodes` | GET | graph | `graph_handlers` | ✅ Implemented |
| `/v1/graph/nodes/:node_id` | `/v1/graph/nodes/{node_id}` | GET | graph | `graph_handlers` | ✅ Implemented |
| `/v1/graph/edges` | `/v1/graph/edges` | POST | graph | `graph_handlers` | ✅ Implemented |
| `/v1/graph/edges` | `/v1/graph/edges` | GET | graph | `graph_handlers` | ✅ Implemented |
| `/v1/graph/nodes/:node_id/edges` | `/v1/graph/nodes/{node_id}/edges` | GET | graph | `graph_handlers` | ✅ Implemented |
| `/v1/graph/artifacts` | `/v1/graph/artifacts` | POST | graph | `ingest_handlers` | ✅ Implemented |
| `/approval-requests/pending` | `/approval-requests/pending` | GET | approvals | `approval_handlers_readonly` | ✅ Implemented |
| `/approval-requests/:approval_request_id/approve` | `/approval-requests/{approval_request_id}/approve` | POST | approvals | `approval_mutation_handlers` | ✅ Implemented |
| `/approval-requests/:approval_request_id/reject` | `/approval-requests/{approval_request_id}/reject` | POST | approvals | `approval_mutation_handlers` | ✅ Implemented |
| `/approval-requests/:approval_request_id/expire` | `/approval-requests/{approval_request_id}/expire` | POST | approvals | `approval_mutation_handlers` | ✅ Implemented |
| `/approval-requests/:approval_request_id/revalidate` | `/approval-requests/{approval_request_id}/revalidate` | GET | approvals | `approval_handlers_readonly` | ✅ Implemented |
| `/approval-requests/trigger-reapproval` | `/approval-requests/trigger-reapproval` | POST | approvals | `trigger_reapproval_handlers` | ✅ Implemented |
| `/policy-snapshots/:snapshot_id` | `/policy-snapshots/{snapshot_id}` | GET | (none) | `policy_snapshot_handlers` | ✅ Implemented |
| `/policy-snapshots/intent/:intent_id/latest` | `/policy-snapshots/intent/{intent_id}/latest` | GET | (none) | `policy_snapshot_handlers` | ✅ Implemented |
| `/policy-snapshots/intent/:intent_id/versions/:version` | `/policy-snapshots/intent/{intent_id}/versions/{version}` | GET | (none) | `policy_snapshot_handlers` | ✅ Implemented |
| `/policy-snapshots/intent/:intent_id` | `/policy-snapshots/intent/{intent_id}` | GET | (none) | `policy_snapshot_handlers` | ✅ Implemented |
| `/forensic/verify` | `/forensic/verify` | POST | forensic | `forensic_handlers` | ✅ Implemented |
| `/forensic/export` | `/forensic/export` | POST | forensic | `forensic_handlers` | ✅ Implemented |
| `/forensic/bundle` | `/forensic/bundle` | POST | forensic | `forensic_handlers` | ✅ Implemented |
| `/forensic/bundles` | `/forensic/bundles` | GET | forensic | `forensic_handlers` | ✅ Implemented |
| `/forensic/bundles/:bundle_id/download` | `/forensic/bundles/{bundle_id}/download` | GET | forensic | `forensic_handlers` | ✅ Implemented |
| `/forensic/bundles/:bundle_id/replay-verify` | `/forensic/bundles/{bundle_id}/replay-verify` | POST | forensic | `forensic_handlers` | ✅ Implemented |

## Drift Detection Rules

1. **Route added but not in OpenAPI:** Add the path to `openapi.yaml` and update this map.
2. **OpenAPI path added but not in router:** Remove from OpenAPI or add the route to `router.rs`.
3. **Method mismatch:** Both router and OpenAPI must agree on the allowed method(s).
4. **Tag mismatch:** Tags should be consistent; new tags must be added to the global `tags:` section in OpenAPI.

## Test Coverage

Route contract tests exist in `crates/intent-api/src/router_smoke_tests.rs` for:
- Forensic endpoints
- ImpactReport endpoint
- Rebase preview/apply endpoints
- Policy snapshot endpoints
- Compensation mutation endpoints

These tests prove routes are wired and reachable. They do not test full handler logic.
