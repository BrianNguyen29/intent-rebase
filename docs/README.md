# Intent Rebase Engine — Production Kit

Bộ tài liệu này là một **implementation package ở mức production** cho dự án **Intent Rebase Engine (IRE)**: hệ thống quản lý phiên bản mục tiêu của người dùng, phát hiện thay đổi có ý nghĩa, tính tác động tới workflow/outputs/approvals/memory đang chạy, và **rebase** thực thi thay vì reset mù hoặc tiếp tục chạy theo intent cũ.

## Mục đích của bộ tài liệu

- Làm **source of truth** cho đội sản phẩm, backend, frontend, platform, security, QA và AI agents tham gia xây dựng hệ thống.
- Giảm mơ hồ khi triển khai bằng cách tách rõ:
  - mục tiêu, non-goals, giá trị cốt lõi
  - kiến trúc production
  - data model, API, events, dataflow
  - bảo mật, audit, quan sát hệ thống, vận hành
  - kế hoạch thực thi theo phase và checklist nghiệm thu
- Hỗ trợ xây dựng hệ thống có thể **mở rộng về tính năng, tổ chức, quy mô và tenant**.

## Nguyên lý cốt lõi

Intent Rebase Engine không thay thế agent runtime, planner hay workflow engine.
Nó là **control layer cho intent**:

1. Chuẩn hóa intent thành cấu trúc có version
2. Tính semantic diff giữa các version
3. Dựng dependency graph giữa intent và artifacts/thực thi
4. Tính invalidation / review / compensation
5. Sinh repair plan / rebased execution plan
6. Gắn provenance để biết output nào được sinh dưới intent version nào

## Khuyến nghị triển khai

- **Backend core**: Rust
- **Workflow/runtime**: Temporal hoặc runtime tương đương
- **OLTP**: Postgres
- **Event / streaming**: Kafka hoặc NATS JetStream
- **Search / analytics**: ClickHouse hoặc OpenSearch
- **Object store**: S3-compatible
- **Frontend**: Next.js + TypeScript
- **Infra**: Kubernetes

## Cấu trúc thư mục

- `01-product/`: product thesis, mục tiêu, use cases, glossary
- `02-architecture/`: hệ thống tổng thể, components, scaling, deployment
- `03-spec/`: intent model, semantic diff, dependency graph, rebase algorithm
- `04-api/`: REST API, event contracts, webhooks
- `05-data/`: schema, storage, dataflow, lifecycle
- `06-backend/`: service boundaries, queues, consistency, failure handling
- `07-frontend/`: console, UX, operator workflows
- `08-security/`: threat model, authn/authz, privacy, audit
- `09-operations/`: environments, CI/CD, observability, SRE, runbooks
- `10-delivery/`: roadmap, phase plans, staffing, checklists
- `11-quality/`: testing, evals, acceptance, UAT
- `12-agents/`: implementation guide cho AI agents
- `13-adrs/`: Architecture Decision Records (ADR pack)
- `14-governance/`: Audit & Governance pack (audit, provenance, policy, compliance)
- `99-reference/`: tham chiếu kỹ thuật và rationale

## Các bộ tài liệu chuyên đề

### ADR Pack (`13-adrs/`)
Bộ ADR ghi lại các quyết định kiến trúc quan trọng: runtime adapter (Temporal), data plane (Postgres+S3), external API (REST), event broker (NATS), observability baseline, rule pack versioning, và approval scope canonicalization.
→ Xem: [13-adrs/README.md](./13-adrs/README.md)

### Implementation Checklist Pack (`10-delivery/checklists/`)
Checklist exit gates cho từng phase (Phase 0–4). Mỗi phase có danh sách items chi tiết với evidence requirements, đảm bảo chất lượng trước khi chuyển phase.
→ Xem: [10-delivery/checklists/README.md](./10-delivery/checklists/README.md)

### Audit & Governance Pack (`14-governance/`)
Bộ tài liệu về audit, governance, compliance: audit event spec, provenance spec, policy snapshot spec, approval scope & revalidation, immutable retention/tamper resistance, threat model v2, authz matrix, tenant isolation, data handling/redaction, forensic bundle, incident freeze, replay compatibility, residual risk spec.
→ Xem: [14-governance/README.md](./14-governance/README.md)

## Tài liệu nên đọc theo thứ tự

1. `01-product/01-product-thesis.md`
2. `01-product/02-goals-nongoals.md`
3. `02-architecture/01-system-overview.md`
4. `03-spec/01-intent-model.md`
5. `03-spec/04-rebase-engine.md`
6. `04-api/01-rest-api.md`
7. `05-data/01-schema.md`
8. `08-security/01-threat-model.md`
9. `10-delivery/01-roadmap.md`
10. `12-agents/01-agent-implementation-guide.md`

## Tài liệu chuyên đề (theo nhu cầu)

### Bắt đầu dự án (Phase 0)
- [ADR Pack README](./13-adrs/README.md) — Hiểu các quyết định kiến trúc đã được đánh giá trước khi bắt đầu implementation
- [Implementation Checklist: Phase 0](./10-delivery/checklists/checklist-phase-0.md) — Exit gate checklist để bắt đầu Phase 1

### Triển khai theo Phase
- [10-delivery/checklists/](./10-delivery/checklists/README.md) — Tất cả phase checklists từ Phase 0 đến Phase 4
- [Current Project Status](./10-delivery/00-current-status.md) — snapshot trạng thái hiện tại của dự án (đã deliver gì, còn gì open)
- [10 Completion Proposals Tracker](./10-delivery/09-completion-proposals-tracker.md) — danh sách 10 proposal còn lại để hoàn thành dự án
- [Intent Rebase Engine vs. Ferrum-Gate Comparison](./10-delivery/12-ferrum-gate-comparison.md) — positioning comparison between intent-rebase (intent lifecycle control plane) and ferrum-gate (tool-call governance/execution gate)
- [Novelty Roadmap](./10-delivery/13-novelty-roadmap.md) — proposed differentiated feature extensions (N1–N6) for intent-rebase with anti-duplication gates vs ferrum-gate and prioritized phase recommendations

### Security & Compliance
- [Audit & Governance Pack README](./14-governance/README.md) — Toàn bộ specs về audit, provenance, policy snapshots, authorization, tenant isolation, forensic bundles, và incident response
- [14-governance/01-audit-event-spec.md](./14-governance/01-audit-event-spec.md) — Audit event schema và integrity chain
- [14-governance/06-threat-model-v2.md](./14-governance/06-threat-model-v2.md) — Threat model đầy đủ (cập nhật từ Phase 1)
- [14-governance/07-authz-matrix.md](./14-governance/07-authz-matrix.md) — RBAC matrix chi tiết
