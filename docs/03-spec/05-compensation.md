# Compensation Model

## Tại sao cần
Không phải mọi task đều thuần tính toán. Nhiều workflow có side effects:
- gửi email
- mở PR
- deploy
- tạo ticket
- sửa DB
- post message ra channel
- approve / reject giao dịch

Nếu intent đổi sau khi side effect xảy ra, chỉ invalidate artifact là chưa đủ.

## Side effect classes

### S0 — Pure read
Không cần compensation.

### S1 — Internal reversible
Ví dụ ghi metadata nội bộ có thể rollback transactionally.

### S2 — External reversible
Ví dụ tạo ticket rồi có thể close/cancel; mở PR rồi có thể close.

### S3 — External partially reversible
Ví dụ gửi email có thể follow-up correction, nhưng không thu hồi tuyệt đối.

### S4 — Irreversible
Ví dụ chuyển tiền, công bố public, xóa dữ liệu không backup.

## Compensation record

```yaml
compensation_id: uuid
side_effect_id: uuid
feasibility: automatic|semi_automatic|manual_only|not_possible
strategy_type: rollback|counter_action|followup_notice|quarantine|escalation
required_approvals: [approval_rule_ref]
generated_at: timestamp
status: pending|approved|executed|failed|waived
```

## Rules
- S0: bỏ qua
- S1: auto nếu policy cho phép
- S2: auto hoặc semi-auto tùy risk
- S3: operator review mặc định
- S4: escalation bắt buộc

## UI requirements
Operator phải thấy:
- side effect nào đã xảy ra
- intent change nào làm nó trở nên problematic
- phương án bù được đề xuất
- residual risk sau compensation
