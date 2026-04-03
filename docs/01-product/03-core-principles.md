# Core Principles

## 1. Intent-first, not prompt-first
Prompt là phương tiện truyền lệnh; intent là đối tượng cần quản lý dài hạn.

## 2. Repair before restart
Chỉ restart toàn bộ khi repair hoặc compensation không an toàn / không khả thi.

## 3. Explicit provenance
Mỗi artifact phải biết nó dựa trên:
- intent version nào
- inputs nào
- policy snapshot nào
- agent/runtime nào

## 4. Explainable invalidation
Khi đánh dấu artifact là invalid hoặc review-required, hệ phải đưa lý do rõ ràng.

## 5. Side-effect awareness
Đọc/viết/approval/external call là các loại hành vi có mức độ rủi ro khác nhau; rebase không thể chỉ nhìn text diff.

## 6. Human override by design
Operator phải luôn có đường:
- approve repair plan
- force restart
- force manual handoff
- suppress low-risk invalidations
- quarantine risky branch

## 7. Event-sourced control history
Mọi thay đổi quan trọng phải được ghi thành events để replay và forensic analysis.

## 8. Multi-tenant and policy-safe
Không để tenant A nhìn thấy graph hoặc artifact nội bộ của tenant B.
