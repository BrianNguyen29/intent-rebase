# Capability Support Matrix

> **Repo:** Intent Rebase Engine (IRE)  
> **Status:** Non-production / Integration-ready

This matrix distinguishes what is currently implemented as bounded slices, what is targeted for upcoming phases, and what is required for production status.

| Capability | Current Bounded Support | Target Phase | Production Status |
|------------|------------------------|--------------|-------------------|
| Intent versioning & semantic diff | Delivered (Phase 1) | — | Bounded — needs load/pen validation |
| Dependency graph CRUD | Delivered (Phase 1) | — | Bounded — needs scale testing |
| Rebase preview & apply | Delivered (Phase 2b) | — | Bounded — manual review path blocked for HIGH/CRITICAL |
| Side-effect ledger & capture-on-write | Delivered (Phase 3 Batch 1) | — | Bounded — artifact-ingest only |
| Compensation action CRUD + bounded executors | Delivered (Phase 3 Batch 1) | — | Bounded — stub executor; real rollback deferred |
| Batch orchestration (approve/reapprove/execute) | Delivered (Phase 3 Batch 1) | — | Bounded — no background worker |
| Policy gate evaluation | Delivered (Phase 3 Batch 1) | — | Bounded — derived from existing fields |
| Orchestration dashboard & dry-run | Delivered (Phase 3 Batch 1) | — | Bounded — read-only |
| Single-shot orchestration runtime | Delivered (Phase 3 Batch 1) | — | Bounded — HTTP + CLI, no scheduler |
| Compensation simulation (N4-4) | Delivered (Phase 3 Batch 1) | — | Bounded — mock executors only |
| Forensic bundle verification | Delivered (Phase 3 Batch 3b) | — | Bounded — read-only feasibility check |
| Forensic bundle generation & download | Delivered (Phase 3 Batch 3b) | — | Bounded — in-memory default; S3 env-gated |
| Forensic replay-verify (integrity hashes) | Delivered (Phase 3 Batch 3b) | — | Bounded — hash verification only, not full replay |
| Tenant isolation & RLS wiring | Bounded Delivered | Phase 4 | Partial — handler-level and app-level RLS tx delivered; full enforcement pending |
| NATS consumer lifecycle + DLQ metrics | Bounded Delivered (Phase 4 first slice) | Phase 4 | Bounded — single consumer + DLQ gauges; replay worker deferred |
| Full runtime replay | Not delivered | Phase 4+ | Deferred |
| S3 Object Lock / immutable storage | Not delivered | Phase 4+ | Deferred |
| Full authentication / authorization | JWT + RLS bounded | Phase 4 | Partial — JWT auth delivered; full authz deferred |
| Production SRE sign-off | Not obtained | Phase 3 exit | Required before production claim |
| External security review | Not obtained | Phase 3 exit | Required before production claim |
| Load testing (L3–L5) | WAIVED-SOLO | Phase 4 | L1–L4 bounded local only; staged/production required |
| Penetration testing | WAIVED-SOLO | Phase 4 | Scope defined; execution required before production |

**Legend**

- **Delivered** — bounded slice implemented and committed
- **Partial / WAIVED-SOLO** — bounded work done but incomplete or pending external validation
- **Not delivered / Deferred** — future phase scope
