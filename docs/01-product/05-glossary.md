# Glossary

- **Intent**: biểu diễn có cấu trúc của mục tiêu, ràng buộc và tiêu chí hoàn thành.
- **Intent Version**: phiên bản của intent sau mỗi thay đổi đáng kể.
- **Semantic Diff**: khác biệt có ý nghĩa giữa hai intent versions.
- **Trace Edge**: liên kết giữa một clause/field của intent với artifact hoặc hành động thực thi.
- **Artifact**: bất kỳ đầu ra hoặc trạng thái trung gian nào của workflow: plan, patch, test, summary, approval, report.
- **Invalidation**: đánh dấu một artifact hoặc task không còn đáng tin dưới intent mới.
- **Review Required**: artifact chưa chắc sai, nhưng cần con người hoặc hệ rules xác minh lại.
- **Compensation**: hành động bù/undo/mitigate cho side effect đã xảy ra.
- **Repair Plan**: kế hoạch sửa cục bộ workflow sau khi intent đổi.
- **Rebase Plan**: kết quả cuối cùng mô tả cách chuyển execution từ intent cũ sang intent mới.
- **Side Effect**: hành động tác động ra ngoài hệ, ví dụ ghi DB, gửi mail, merge PR, gọi API thay đổi trạng thái.
- **Policy Snapshot**: ảnh chụp chính sách hiệu lực tại thời điểm một artifact hoặc action được tạo.
- **Checkpoint**: trạng thái bền vững cho phép resume workflow.
