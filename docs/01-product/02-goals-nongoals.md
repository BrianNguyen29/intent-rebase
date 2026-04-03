# Goals and Non-Goals

## Goals

### G1. Chuẩn hóa intent thành đối tượng có version
Intent không chỉ là đoạn chat. IRE phải hỗ trợ:
- objective
- constraints
- acceptance criteria
- trust / approval boundaries
- budget / latency / cost caps
- prohibited actions
- preferred trade-offs
- external references

### G2. Tính semantic diff có ý nghĩa
Hệ thống phải nhận diện được các loại thay đổi khác nhau như:
- bổ sung chi tiết
- thu hẹp hoặc mở rộng scope
- thay đổi ràng buộc
- thay đổi chất lượng/definition of done
- thay đổi authority scope
- thay đổi budget hoặc urgency
- thay đổi risk appetite

### G3. Dựng dependency graph giữa intent và execution artifacts
Graph phải nối được:
- intent clauses
- plans / tasks
- agent runs
- tool invocations
- outputs
- tests
- approvals
- side effects
- memory items

### G4. Rebase thay vì restart mặc định
Hệ thống phải ưu tiên:
- salvage phần còn đúng
- invalidate có chọn lọc
- xin review/approval lại khi cần
- compensation cho side effects
- resume từ checkpoint hợp lệ

### G5. Audit và replay đầy đủ
Phải trả lời được:
- thay đổi gì đã xảy ra
- ai/đâu đã tạo thay đổi
- artifact nào bị ảnh hưởng
- tại sao hệ chọn repair/restart/compensate
- output nào sinh dưới intent version nào

### G6. Production readiness
Hệ thống phải có:
- authn/authz rõ ràng
- audit logs
- multi-tenant isolation
- SLA / SLO
- observability
- runbooks
- backpressure / retry / idempotency

## Non-Goals

### NG1. Không phải một LLM model platform
IRE không huấn luyện model riêng, không thay inference provider.

### NG2. Không thay thế workflow engine
IRE nên tích hợp với Temporal/LangGraph/custom runtime thay vì tái phát minh durable execution từ đầu.

### NG3. Không là source-of-truth duy nhất cho business specs
IRE tiêu thụ specs từ nhiều nguồn và chuẩn hóa thành Intent Objects; spec gốc vẫn có thể sống ở Git, issue tracker, ticketing, docs.

### NG4. Không tự động undo mọi side effect
Nhiều side effect ngoài đời thực không thể đảo ngược hoàn hảo; hệ thống phải phân loại và escalate.

### NG5. Không phụ thuộc duy nhất vào một hệ agent
Thiết kế phải hỗ trợ nhiều runtime/protocol khác nhau qua adapters.
