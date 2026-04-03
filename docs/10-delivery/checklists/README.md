# Implementation Checklist Pack

## Mục đích

Bộ checklist này định nghĩa các **exit gates** cho từng phase của Intent Rebase Engine. Mỗi checklist là **bắt buộc** trước khi chuyển sang phase tiếp theo.

---

## Chỉ mục Checklist

| Checklist | Phase | Exit Gate Criteria |
|-----------|-------|-------------------|
| [Phase 0 — Foundations](./checklist-phase-0.md) | Phase 0 | Repo scaffolded, ADRs accepted, architecture baseline, local dev ready, CI green |
| [Phase 1 — Core Control Plane MVP](./checklist-phase-1.md) | Phase 1 | Intent schema v1, semantic diff v1, graph model v1, rebase preview, audit baseline |
| [Phase 2 — Runtime-Integrated Rebase](./checklist-phase-2.md) | Phase 2 | Runtime adapter v1, checkpoint mapping, apply rebase for low/medium risk, approvals revalidation |
| [Phase 3 — Compensation + Hardening](./checklist-phase-3.md) | Phase 3 | Side effect ledger, compensation engine, SRE/observability, tenant isolation, forensic replay |
| [Phase 4 — Enterprise Expansion](./checklist-phase-4.md) | Phase 4 | Policy simulation, advanced adapters, cross-workflow families, trust scoring |

---

## Quy tắc chung

1. **Mỗi checkbox phải có evidence** trước khi đánh dấu complete:
   - PR merged và reviewed
   - Test coverage ≥ 80% cho module mới
   - Metrics dashboard available và showing green
   - Security review signed off (cho items liên quan security)
2. **No partial passes** — exit gate chỉ pass khi tất cả items checked
3. **Blocking issues** phải được resolve trước khi proceed
4. **Docs must be updated** khi code changes affect documented behavior

---

## Liên kết nội bộ

- **Roadmap:** `../01-roadmap.md`
- **Phase descriptions:** `../02-phase-0-foundations.md`, `../03-phase-1-core-control-plane.md`, `../04-phase-2-runtime-integrated.md`, `../05-phase-3-hardening.md`, `../06-phase-4-expansion.md`
- **ADR Pack:** `../../13-adrs/README.md`
- **Agent Guide:** `../../12-agents/01-agent-implementation-guide.md`
- **Governance Pack:** `../../14-governance/README.md`

---

## Definition of Done cho mỗi Item

```
[ ] Item description
    Evidence:
    - PR: <link>
    - Tests: <coverage or test count>
    - Docs: <updated doc link>
    - Metrics: <dashboard link showing green>
    - Security sign-off: <reviewer + date>
```