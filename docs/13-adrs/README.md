# ADR Pack — Architecture Decision Records

## Purpose

The ADR pack records key architectural decisions that have been evaluated, discussed, and resolved for the Intent Rebase Engine. Each ADR includes context, the decision, consequences, and status.

**Status convention:** `Proposed` → `Accepted` → `Deprecated` → `Superseded`

---

## ADR Index

| ID | Title | Status | Phase |
|----|---------|-----------|-------|
| [ADR-01](./01-runtime-adapter.md) | Runtime Adapter Selection | **Accepted** | P0–P1 |
| [ADR-02](./02-data-plane.md) | Data Plane Architecture | **Accepted — Partially implemented** | P0–P1 |
| [ADR-03](./03-external-api.md) | External API Protocol | **Accepted — Partially implemented** | P0–P1 |
| [ADR-04](./04-event-broker.md) | Event Broker Selection | **Accepted — Partially implemented** | P0–P1 |
| [ADR-05](./05-observability-baseline.md) | Observability Baseline | **Accepted — Partially implemented** | P0–P1 |
| [ADR-06](./06-rule-pack-versioning.md) | Rule Pack Versioning | **Accepted — Partially implemented** | P0–P1 |
| [ADR-07](./07-approval-scope-canonicalization.md) | Approval Scope & Policy Snapshot Canonicalization | **Accepted — Partially implemented** | P1 |
| [ADR-08](./08-artifact-side-effect-tx-boundary.md) | Artifact Side-Effect Transaction Boundary | **Accepted — Option A bounded implemented for SQL/RLS ingest path; non-RLS fallback preserved** | P2 |
| [ADR-09](./09-rebase-apply-rls-transaction-boundary.md) | Rebase Apply RLS Transaction Boundary | **Accepted — Bounded D1–D7 implemented at commit `d98c7dc`** | Phase 4 |
| [ADR-10](./10-impact-report-design.md) | ImpactReport Design | **Accepted — bounded MVP implemented; no persistence, no migration, no production-ready claim** | Phase 2 |
| [ADR-11](./11-policy-config-rebase-pillar.md) | Policy / Config Rebase Pillar — MVP Design | **Accepted — bounded MVP implemented; no persistence, no migration, no production-ready claim** | Phase 3 |
| [ADR-12](./12-workflow-migration-rebase.md) | Workflow Migration / Rebase Pillar — Phase 4 Design | **Proposed — design-only; no implementation, no persistence, no production-ready claim** | Phase 4 |
| [ADR-13](./13-webhook-outbox-version-persistence.md) | Webhook Outbox Version Persistence | **Accepted — bounded implemented; no production readiness claim** | Phase 4a |
| [ADR-14](./14-forensic-chain-hash.md) | Forensic Chain-Hash Algorithm and Linking Protocol | **Accepted — local algorithm implemented; Object Lock/production deferred** | Phase 4 |
| [ADR-15](./15-nats-per-tenant-streams.md) | NATS Per-Tenant JetStream Stream Migration Strategy | **Proposed — design-only; implementation blocked on external gates** | Phase 4 |
| [ADR-16](./16-solo-private-operation-waiver.md) | Solo Private-Only Operation Waiver and Self-Attestation | **Accepted — Solo Private-Only** | Phase 3–4 |

---

## Internal Links

- **Roadmap:** `../10-delivery/01-roadmap.md`
- **Agent Guide:** `../12-agents/01-agent-implementation-guide.md`
- **Architecture:** `../02-architecture/01-system-overview.md`
- **Threat Model:** `../14-governance/06-threat-model-v2.md` (current) — see also `../08-security/01-threat-model.md` (baseline/legacy)
- **Governance Pack:** `../14-governance/README.md`

## Contributing a New ADR

1. Create a new file following the `NN-title-slug.md` pattern in this directory
2. Use the standard template: **Context → Decision → Consequences**
3. Mark as `Proposed` until reviewed and accepted by the team
4. Update the index table in this file
5. Link from related ADRs (see `## Related ADRs` in each file)