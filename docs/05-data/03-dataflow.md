# Dataflow

## Flow 1 — Intent creation
1. Source gửi payload
2. Ingestion validate + normalize
3. Intent version v1 được ghi
4. Event `intent.created`
5. Initial trace anchors được tạo

## Flow 2 — Intent change + rebase preview
1. Source thay đổi spec/request
2. Intent version mới được tạo
3. Diff worker tính semantic diff
4. Impact engine query graph
5. Rebase planner sinh preview
6. Console / webhook thông báo review

## Flow 3 — Rebase apply
1. Operator hoặc rule engine approve
2. Runtime adapter pause execution
3. Rebase plan được apply
4. Artifacts invalidated/quarantined
5. Compensation tasks inserted nếu cần
6. Workflow resume từ checkpoint
7. Event `workflow.rebased`

## Flow 4 — Audit export
1. Incident được chọn
2. Replay service gom event timeline + artifacts + provenance
3. Export bundle sinh ra object store
4. Operator tải bundle

## Flow 5 — Approval stale detection
1. Policy update hoặc intent change xảy ra
2. Approval evaluator chạy rules
3. Approval cũ bị đánh dấu stale
4. Bước side effect bị block
