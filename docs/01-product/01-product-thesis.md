# Product Thesis

## Tuyên bố sản phẩm

**Intent Rebase Engine (IRE)** là lớp runtime/control plane quản lý sự thay đổi của mục tiêu người dùng trong khi agent workflows đang chạy. IRE phát hiện thay đổi có ý nghĩa ở intent, xác định ảnh hưởng lên plan, outputs, approvals, side effects và memory, sau đó tạo **rebase plan** để workflow được sửa an toàn, có thể audit và resume từ trạng thái hợp lệ gần nhất.

## Vấn đề thị trường

Các hệ agent đang mạnh lên ở 3 hướng:
- tác vụ kéo dài nhiều phút đến nhiều giờ
- nhiều sub-agent / nhiều tool / nhiều bước side effect
- spec-driven workflows và human-in-the-loop

Điểm gãy lớn nhất không chỉ là hallucination, mà là **intent drift ở tầng hệ thống**:
- người dùng đổi ý giữa chừng
- policy / budget / approval boundary thay đổi
- spec / ticket / PR requirements thay đổi
- hệ vẫn tiếp tục trên intent cũ hoặc reset toàn bộ

Đó là lỗ hổng dẫn đến:
- code đi sai hướng
- support/ops actions dùng policy cũ
- approvals không còn hợp lệ
- memory/context bị giữ lại sai chỗ
- root-cause mơ hồ khi có incident

## Định vị

IRE là:
- **version control cho intent**
- **change impact engine cho agent workflows**
- **repair / compensation orchestrator** cho thay đổi intent

IRE không phải:
- một generic LLM gateway
- một agent framework đầy đủ
- một workflow engine thuần
- một memory database thuần
- một observability tool thuần

## Giá trị cốt lõi

### 1. Giảm chi phí reset
Không rerun toàn bộ workflow khi intent chỉ thay đổi cục bộ.

### 2. Giảm rủi ro hành động sai
Khi intent đổi, approvals, policies và side effects được đánh giá lại có cấu trúc.

### 3. Tăng độ tin cậy
Mọi output đều có provenance theo intent version.

### 4. Tăng năng suất của agent teams
Agent có thể salvage analysis, tests, artifacts và chỉ rerun phần bị ảnh hưởng.

### 5. Tăng khả năng kiểm soát của con người
Người vận hành nhìn thấy rõ:
- thay đổi nào vừa xảy ra
- phần nào còn hợp lệ
- phần nào bị invalid
- phần nào cần review / compensation / approval lại

## Wedge triển khai ban đầu

Ngành dọc phù hợp nhất cho MVP:
- AI coding / software delivery
- internal ops workflows
- document review / policy-aware agents
- long-running research pipelines

Trong ngắn hạn, ưu tiên **coding agents** vì:
- intent thường thay đổi giữa chừng
- artifacts có cấu trúc rõ: plan, patch, tests, PR, approvals
- dễ đo ROI: giảm rerun, giảm churn, giảm sai lệch spec

## Tuyên bố thành công

Một hệ IRE production thành công khi:
- thay đổi intent được formalize, diff được, trace được
- workflow không bị reset bừa bãi
- approvals/policies được re-evaluate khi cần
- side effects được phân loại compensatable/non-compensatable
- incident có thể replay và giải thích
