# Implementation Checklist Pack

## Purpose

This checklist pack defines the **exit gates** for each phase of the Intent Rebase Engine. Each checklist is **mandatory** before moving to the next phase.

---

## Checklist Index

| Checklist | Phase | Exit Gate Criteria |
|-----------|-------|-------------------|
| [Phase 0 — Foundations](./checklist-phase-0.md) | Phase 0 | Repo scaffolded, ADRs accepted, architecture baseline, local dev ready, CI green |
| [Phase 1 — Core Control Plane MVP](./checklist-phase-1.md) | Phase 1 | Intent schema v1, semantic diff v1, graph model v1, rebase preview, audit baseline |
| [Phase 2 — Runtime-Integrated Rebase](./checklist-phase-2.md) | Phase 2 | Runtime adapter v1, checkpoint mapping, apply rebase for low/medium risk, approvals revalidation |
| [Phase 3 — Compensation + Hardening](./checklist-phase-3.md) | Phase 3 | Side effect ledger, compensation engine, SRE/observability, tenant isolation, forensic replay |
| [Phase 4 — Enterprise Expansion](./checklist-phase-4.md) | Phase 4 | Policy simulation, advanced adapters, cross-workflow families, trust scoring |

---

## General Rules

1. **Every checkbox must have evidence** before being marked complete:
   - PR merged and reviewed
   - Test coverage ≥ 80% for new module
   - Metrics dashboard available and showing green
   - Security review signed off (for security-related items)
2. **No partial passes** — exit gate only passes when all items are checked
3. **Blocking issues** must be resolved before proceeding
4. **Docs must be updated** when code changes affect documented behavior

---

## Internal Links

- **Roadmap:** `../01-roadmap.md`
- **Phase descriptions:** `../02-phase-0-foundations.md`, `../03-phase-1-core-control-plane.md`, `../04-phase-2-runtime-integrated.md`, `../05-phase-3-hardening.md`, `../06-phase-4-expansion.md`
- **ADR Pack:** `../../13-adrs/README.md`
- **Agent Guide:** `../../12-agents/01-agent-implementation-guide.md`
- **Governance Pack:** `../../14-governance/README.md`

---

## Definition of Done for Each Item

```
[ ] Item description
    Evidence:
    - PR: <link>
    - Tests: <coverage or test count>
    - Docs: <updated doc link>
    - Metrics: <dashboard link showing green>
    - Security sign-off: <reviewer + date>
```