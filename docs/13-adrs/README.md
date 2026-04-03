# ADR Pack — Architecture Decision Records

## Mục đích

Bộ ADR ghi lại các quyết định kiến trúc quan trọng đã được đánh giá, thảo luận và resolved cho Intent Rebase Engine. Mỗi ADR bao gồm context, lựa chọn, hệ quả và trạng thái.

**Trạng thái qui ước:** `Proposed` → `Accepted` → `Deferred` → `Deprecated` → `Superseded`

---

## Chỉ mục ADR

| ID | Tiêu đề | Trạng thái | Phase |
|----|---------|-----------|-------|
| [ADR-01](./01-runtime-adapter.md) | Runtime Adapter Selection | **Accepted** | P0–P1 |
| [ADR-02](./02-data-plane.md) | Data Plane Architecture | **Accepted** | P0–P1 |
| [ADR-03](./03-external-api.md) | External API Protocol | **Accepted** | P0–P1 |
| [ADR-04](./04-event-broker.md) | Event Broker Selection | **Deferred** (interim default: NATS JetStream) | P0–P1 |
| [ADR-05](./05-observability-baseline.md) | Observability Baseline | **Accepted** | P0–P1 |
| [ADR-06](./06-rule-pack-versioning.md) | Rule Pack Versioning | **Accepted** | P0–P1 |
| [ADR-07](./07-approval-scope-canonicalization.md) | Approval Scope & Policy Snapshot Canonicalization | **Accepted** | P1 |

---

## Liên kết nội bộ

- **Roadmap:** `../10-delivery/01-roadmap.md`
- **Agent Guide:** `../12-agents/01-agent-implementation-guide.md`
- **Architecture:** `../02-architecture/01-system-overview.md`
- **Threat Model:** `../08-security/01-threat-model.md`
- **Governance Pack:** `../14-governance/README.md`

## Hướng dẫn đóng góp ADR mới

1. Tạo file mới theo pattern `NN-title-slug.md` trong thư mục này
2. Sử dụng template chuẩn: **Context → Decision → Consequences**
3. Đánh dấu `Proposed` cho đến khi được team review và accept
4. Cập nhật bảng chỉ mục trong file này
5. Liên kết từ ADR liên quan (xem `## Related ADRs` ở mỗi file)