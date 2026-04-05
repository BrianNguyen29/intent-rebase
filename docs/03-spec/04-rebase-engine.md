# Rebase Engine Specification

## Phase 1 Status (PR #15 + PR #16 + PR #17 + PR #18)

**PR #15 — Preview Endpoint:**
- Decision classes A-E mapping from diff+risk analysis
- `RebasePlan` output type with typed decision class, rationale, and section decisions
- Deterministic decision class computation (same input → same output)
- `RebaseEngine::generate_plan()` for plan generation
- Rebase preview HTTP endpoint: POST /v1/intents/{id}/rebase-preview

**PR #16 — Graph-Integrated Affected Items:**
- `AffectedItemsPreview` with `status` field (available/unavailable)
- Graph-based impact classification via `classify_impact()` from dependency graph
- `affected_items` list in preview response when graph data is available
- Endpoint remains functional even when graph coverage is incomplete
- Safe default: classification starts from target IntentVersion graph node (to_version)

**PR #17 — Apply/Checkpoint Groundwork:**
- Typed internal contract for checkpoint selection readiness (`CheckpointSelection`)
- Typed internal contract for approval revalidation readiness (`ApprovalRevalidation`)
- Typed internal contract for compensation action readiness (`CompensationReadiness`)
- `DeferredFields` replaced stringly TODO placeholders with structured Phase 1 groundwork types
- New types exported from `rebase_engine`: `CheckpointSelection`, `CheckpointCandidate`, `ApprovalRevalidation`, `ApprovalNeedingRevalidation`, `RevalidationStrategy`, `CompensationReadiness`, `CompensationAction`
- All Phase 2 fields have `ready: false` in Phase 1
- Deterministic unit tests verify deferred state properties
- Apply HTTP endpoint NOT added (Phase 2)

**PR #18 — Internal Checkpoint Heuristic Baseline:**
- `CheckpointSelection::heuristic_baseline()` adds deterministic internal checkpoint strategy hints by decision class
- Class C prefers the nearest validated checkpoint before the first invalidated node
- Class D prefers a checkpoint before irreversible side effects when possible
- Class E surfaces a manual-handoff boundary without auto-selecting a restart point
- `CheckpointSelection.ready` remains `false`; no runtime-backed checkpoint execution exists yet

**PR #19 — Internal Approval-Revalidation Heuristic Baseline:**
- `ApprovalRevalidation::heuristic_baseline()` adds deterministic internal strategy hints by decision class
- When graph-derived `affected_approvals` are available (via PR #16 graph integration), they are mapped to `ApprovalNeedingRevalidation` entries for Class C and D
- Class E drops all approvals (clean slate before manual handoff) regardless of graph data
- Class A and B produce no invalidation candidates (no immediate revalidation needed)
- When `affected_approvals` is unavailable, the heuristic falls back to empty approvals with a truthful rationale
- `ApprovalRevalidation.ready` remains `false`; execution deferred to Phase 2 runtime adapter
- No public API or OpenAPI changes; internal-only heuristic baseline

**NOT YET IMPLEMENTED (Phase 2+):**
- Runtime-backed checkpoint discovery and execution
- Approval revalidation execution hooks
- Runtime adapter integration
- Rebase apply (preview-only in Phase 1)
- Compensation action generation (identified in `side_effects`, not generated)

## Mục tiêu
Tạo quyết định có cấu trúc khi intent thay đổi:
- giữ cái gì
- hủy cái gì
- xin lại gì
- bù gì
- resume từ đâu

## Rebase state machine

```text
DetectedChange
  -> DiffComputed
  -> ImpactComputed
  -> (AutoRepairCandidate | ManualReviewRequired)
  -> RebasePlanIssued
  -> Applied
  -> Verified
  -> Closed
```

## Inputs
- old intent version
- new intent version
- change set
- dependency graph snapshot
- runtime state snapshot
- latest approvals
- policy snapshot
- side effect ledger

## Outputs
- rebase decision
- invalidation set
- review set
- compensation set
- restart boundary
- checkpoint resume pointer
- approval requirements
- operator notices

## Decision classes

### Class A — No-op / Metadata update
Thay đổi không ảnh hưởng execution semantics.

### Class B — Soft review
Không invalidate ngay, nhưng cần review trước bước tiếp theo.

### Class C — Partial repair
Invalidate cục bộ, giữ phần còn lại, rerun từ checkpoint chọn lọc.

### Class D — Compensation + repair
Đã có side effect cần bù hoặc mitigate.

### Class E — Hard restart / manual handoff
Không đủ an toàn để auto-repair.

## Rebase algorithm (v1 conceptual)

1. Load intent versions
2. Compute semantic diff
3. Identify impacted clauses
4. Query dependency graph for affected nodes
5. Classify affected nodes by node type and risk policy
6. Detect side-effect class and compensation feasibility
7. Choose candidate checkpoint
8. Re-evaluate approvals and policies
9. Generate rebase plan
10. Optional operator confirmation
11. Apply via runtime adapter
12. Verify resulting execution state

## Checkpoint selection rules
Ưu tiên checkpoint:
- gần nhất
- trước node invalid đầu tiên
- không bỏ sót dependency bắt buộc
- tránh rerun side effects đã irreversible nếu không cần

PR #18 chỉ mới thêm heuristic baseline nội bộ để xếp hạng checkpoint strategy hints theo decision class; mapping sang checkpoint runtime thật vẫn deferred tới Phase 2.

## Repair primitives
- drop_task(node)
- rescope_task(node, intent_delta)
- regenerate_artifact(node)
- request_approval(rule)
- insert_validation_step(type)
- insert_compensation(step)
- branch_execution(reason)
- quarantine_output(artifact_id)

## Safety rails
- Không auto-apply với critical changes nếu adapter không hỗ trợ safe pause/resume
- Không auto-compensate irreversible side effects
- Không reuse stale approvals
- Không resume nếu runtime state và graph state không đồng bộ

## Success metrics
- rebase acceptance rate
- percentage work salvaged
- percentage full restarts avoided
- invalidation precision/recall (estimated via review labels)
- incident reduction due to stale intent
