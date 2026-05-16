# Agent Safety Rebase Roadmap

> **Repo:** Intent Rebase Engine (IRE)  
> **Status:** Non-production / Integration-ready  
> **Last Updated:** 2026-05-12

## Product Positioning

Agent Safety Rebase is the control-plane layer for intent change in automated decision-maker and workflow-runner systems. It provides policy/config rebase, workflow migration/rebase, and a multi-tenant compliance automation foundation.

This repository remains **Intent Rebase Engine** — no repo or package rename.

## Roadmap Phases

### Phase 0 — Positioning Cleanup (Complete)
- Agent Safety Rebase positioning doc
- Capability support matrix
- README / docs index updates

### Phase 1 — Documentation & API Contract Stabilization (Complete)
- Clean stale status wording (local/pending commit)
- Normalize OpenAPI / router forensic paths
- Add route contract tests for forensic endpoints
- Introduce ImpactReport design (ADR-10) and implement bounded MVP read-only projection
- Accept ADR-10 (bounded MVP, no persistence, no migration)
- Add ImpactReport examples (no-impact, approval invalidation, compensation required, manual review)
- Strengthen route/OpenAPI drift guard (contract map + automated test)

### Phase 2 — Agent Safety Core Language & Domain Model (Current)
- Formalize vocabulary: IntentVersion, RebasePlan, ImpactReport, SafetyGate, etc. → see [Glossary](../01-product/05-glossary.md)
- Design ImpactReport as an on-demand read-only projection across pillars (no persistence for MVP)
- Define API contract for impact-report (ADR-10 bounded MVP implemented)
- Define API contract for propagation-status (Slice 1 bounded implemented locally — migration 017, record type, in-memory repo, stub fallback; full downstream tracking deferred to Phase 4+)
- **Checkpoint:** Phase 1/2 local milestone — non-production, integration-ready only

### Phase 3 — Policy / Config Rebase Pillar

**Implemented (bounded MVP — non-production):**
- `GET /policy-snapshots/{snapshot_id}/impact-report` — read-only ImpactReport for a policy snapshot's intent, reusing ADR-10 semantics (ADR-11 accepted)

**Deferred to Phase 4+:**
- Full `PolicyRebaseAdapter` interface and implementation
- Config object model and schema registry
- Policy/config rebase preview and apply examples
- Environment promotion semantics (dev/staging/prod)

### Phase 4 — Workflow Migration / Rebase Pillar

**Design-only (ADR-12 Proposed):**
- `WorkflowRebaseAdapter` interface design for workflow-to-intent diff propagation
- `GET /intents/{intent_id}/propagation-status` — Slice 1 bounded implemented locally (migration 017, `PropagationRecord` domain type, in-memory repository, record-backed response when repo available, stub fallback when `None`); webhook delivery, event streaming, and cross-workflow lineage deferred to Phase 4+
- API contract design for workflow migration preview / apply / status endpoints

**Deferred implementation (Phase 4+):**
- Runtime adapter contract and capability registry
- Workflow migration preview / apply / status APIs
- Cross-workflow lineage and DLQ replay hardening

> See also:
> - [Main Roadmap](./01-roadmap.md) for high-level Phase 4 enterprise expansion items
> - [Propagation Status Implementation Plan](./19-propagation-status-implementation-plan.md) for concrete staged slices

### Phase 5 — Multi-Tenant Compliance Automation Foundation
- Full RLS audit with non-bypass role
- Immutable evidence roadmap (S3 Object Lock, chain-hash, retention)
- Forensic replay hardening and tamper tests

### Phase 6 — Production Readiness Path
- External SRE sign-off
- External security review and pen test
- L3–L5 load testing
- Production infrastructure and real alerting

## Out of Scope

- LLM gateway / agent runtime / tool-call executor / MCP bridge
- Full AI agent framework (future adapter repo)
- Production-ready claims before external gates close

## Related Docs

- [Product Positioning](../01-product/03-agent-safety-rebase-positioning.md)
- [Capability Support Matrix](../01-product/04-capability-support-matrix.md)
- [Current Status](./00-current-status.md)
- [Production Readiness Backlog](./17-production-readiness-backlog.md)
