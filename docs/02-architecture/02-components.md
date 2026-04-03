# Components

## 1. Intent Ingestion Service
### Nhiệm vụ
- nhận sources: chat, markdown spec, issue comment, webhook, API call
- normalize sang Intent DTO
- validate schema
- enrich metadata: actor, source, timestamps, tenant, workflow refs

### Interface
- REST API
- Git provider webhooks
- ticketing connectors
- policy system events

## 2. Intent Registry
### Nhiệm vụ
- lưu current intent và lịch sử versions
- quản lý lineage giữa versions
- hỗ trợ compare và snapshot retrieval

### Yêu cầu
- immutable version records
- mutable “current head” pointer theo workflow/session
- optimistic concurrency control

## 3. Semantic Diff Engine
### Nhiệm vụ
- so sánh intent versions
- xuất ra machine-readable change set
- gán severity và confidence
- tách low-risk vs high-risk changes

### Output mẫu
- change_type
- affected_fields
- rationale
- confidence
- policy_relevance
- requires_human_confirmation

## 4. Trace Graph Service
### Nhiệm vụ
- quản lý quan hệ giữa intent clauses và artifacts/actions
- query impact radius
- compute transitive dependencies

## 5. Impact Analysis Engine
### Nhiệm vụ
- chạy propagation rules trên graph
- xuất danh sách:
  - still_valid
  - review_required
  - invalid
  - compensatable
  - restart_required

## 6. Rebase Planner
### Nhiệm vụ
- tạo repair plan
- tính checkpoint resume point
- chèn approval steps
- sinh compensation tasks nếu cần

## 7. Runtime Adapter Layer
### Nhiệm vụ
- translate rebase plan sang workflow runtime cụ thể
- pause/resume/cancel/branch execution
- gắn metadata intent_version vào runs

## 8. Policy / Approval Evaluator
### Nhiệm vụ
- xác định approval nào phải xin lại
- kiểm tra authority scope, cost caps, forbidden actions
- evaluate under policy snapshot mới

## 9. Artifact Service
### Nhiệm vụ
- quản lý outputs, patches, summaries, test reports, decision docs
- lưu object payloads ngoài metadata

## 10. Side Effect Ledger
### Nhiệm vụ
- phân loại actions:
  - pure read
  - internal write
  - external reversible
  - external irreversible
- attach compensation strategy

## 11. Audit and Replay Service
### Nhiệm vụ
- ghi event log
- reconstruct timeline
- replay decisions
- xuất forensic bundle

## 12. Operator Console
### Nhiệm vụ
- hiển thị diff intent
- impact map
- rebase preview
- approval UI
- incident timeline
