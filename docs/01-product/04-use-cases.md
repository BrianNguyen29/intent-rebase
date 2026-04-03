# Primary Use Cases

## UC1. Coding Agent Rebase
### Tình huống
Agent đang refactor module auth. Giữa chừng, user đổi yêu cầu:
- giữ backward compatibility
- không sửa public API
- phải tăng test coverage

### Hành vi mong muốn
- giữ analysis về codebase hiện tại
- invalidate patch liên quan public API
- đánh dấu test plan cũ là incomplete
- spawn thêm task cho compatibility tests
- thu hồi approval cũ nếu approval scope thay đổi

## UC2. Support Workflow Rebase
### Tình huống
Agent đang chuẩn bị phản hồi khách hàng dựa trên policy cũ. Policy team vừa thay đổi escalation criteria.

### Hành vi mong muốn
- draft trả lời bị review-required
- bước gửi mail bị block
- approval path được cập nhật
- operator thấy rõ diff policy -> impact

## UC3. Research Workflow Rebase
### Tình huống
Lead agent đang điều phối 5 sub-agents nghiên cứu vendor options. Sau đó, budget bị cắt và security requirement tăng.

### Hành vi mong muốn
- các summary discovery vẫn còn dùng được
- ranking/recommendation outputs bị invalid
- re-run vendor scoring dưới constraints mới
- external RFQ steps chưa gửi thì bị hủy

## UC4. Internal Ops / DevOps Rebase
### Tình huống
Agent đã lên kế hoạch rollout canary. Sau đó SRE đổi error budget policy và freeze window.

### Hành vi mong muốn
- plan bị rebase, không auto-deploy
- change request cần approval mới
- rollback strategy được thêm bắt buộc
