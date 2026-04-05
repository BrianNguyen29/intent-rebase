# Rebase Engine Specification

## Phase 1 Status (PR #14 — Planner Baseline)

**Implemented:**
- Decision classes A-E mapping from diff+risk analysis
- `RebasePlan` output type with typed decision class, rationale, and section decisions
- Deterministic decision class computation (same input → same output)
- `RebaseEngine::generate_plan()` for plan generation

**NOT YET IMPLEMENTED (Phase 2+):**
- Graph-based affected node classification (requires graph HTTP API)
- Checkpoint selection heuristics
- Approval revalidation hooks
- Runtime adapter integration
- Rebase apply/preview HTTP endpoints

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
