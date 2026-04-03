# Phase 0 — Foundations Checklist

**Exit Gate:** Phase 0 complete khi tất cả items bên dưới checked và có evidence.

**Trạng thái:** `IN PROGRESS`  
**Phase:** Phase 0  
**Target Duration:** 2–4 tuần

---

## 1. Product Spec Finalized

```
[x] Product thesis finalized
    Evidence:
    - PR merged: #1 (docs: add v1 baseline with ADR, checklist, and governance packs)
    - Doc: ../../01-product/01-product-thesis.md — exists in docs baseline

[x] Goals and non-goals defined
    Evidence:
    - PR merged: #1
    - Doc: ../../01-product/02-goals-nongoals.md — exists in docs baseline

[x] Core principles documented
    Evidence:
    - PR merged: #1
    - Doc: ../../01-product/03-core-principles.md — exists in docs baseline

[x] Use cases documented
    Evidence:
    - PR merged: #1
    - Doc: ../../01-product/04-use-cases.md — exists in docs baseline

[x] Glossary updated
    Evidence:
    - PR merged: #1
    - Doc: ../../01-product/05-glossary.md — exists in docs baseline
```

---

## 2. Domain Model Finalized

```
[x] Intent model v1 defined
    Evidence:
    - PR merged: #1
    - Doc: ../../03-spec/01-intent-model.md — exists in docs baseline

[x] Semantic diff approach documented
    Evidence:
    - PR merged: #1
    - Doc: ../../03-spec/02-semantic-diff.md — exists in docs baseline

[x] Dependency graph model documented
    Evidence:
    - PR merged: #1
    - Doc: ../../03-spec/03-dependency-graph.md — exists in docs baseline

[x] Rebase algorithm documented
    Evidence:
    - PR merged: #1
    - Doc: ../../03-spec/04-rebase-engine.md — exists in docs baseline

[x] Compensation model documented
    Evidence:
    - PR merged: #1
    - Doc: ../../03-spec/05-compensation.md — exists in docs baseline

[x] Provenance model documented
    Evidence:
    - PR merged: #1
    - Doc: ../../03-spec/06-provenance.md — exists in docs baseline
```

---

## 3. Repository Scaffolding

```
[x] Rust workspace created
    Evidence:
    - File: Cargo.toml (root workspace with members: intent-rebase-types, intent-service, rebase-engine, graph-service)
    - Verified: cargo check --workspace passes

[x] Service structure defined (intent-service, rebase-engine, graph-service)
    Evidence:
    - Directory structure: crates/intent-service/, crates/rebase-engine/, crates/graph-service/
    - Each crate has lib.rs with Phase-1 stub implementation

[x] Shared crate created (intent-rebase-types)
    Evidence:
    - Directory: crates/intent-rebase-types/
    - Contains: intent.rs, artifact.rs, graph.rs, audit.rs, error.rs

[ ] Database migration tool integrated (sqlx-cli or similar)
    Evidence:
    - NOT PRESENT — deferred to Phase 1 (migrations defined in ADR-02 but tooling not bootstrapped)

[x] Docker Compose for local infra (Postgres, NATS, MinIO)
    Evidence:
    - File: infrastructure/local/docker-compose.yml
    - Services: postgres:5432, nats:4222, minio:9000/9001
```

---

## 4. ADRs Accepted

```
[x] ADR-01: Runtime Adapter Selection (Temporal)
    Evidence:
    - Doc: ../../13-adrs/01-runtime-adapter.md
    - Status: Accepted (updated from Proposed)

[x] ADR-02: Data Plane Architecture (Postgres + S3)
    Evidence:
    - Doc: ../../13-adrs/02-data-plane.md
    - Status: Accepted (updated from Proposed)

[x] ADR-03: External API Protocol (REST + Webhooks)
    Evidence:
    - Doc: ../../13-adrs/03-external-api.md
    - Status: Accepted (updated from Proposed)

[x] ADR-04: Event Broker Selection — Deferred (NATS or Kafka)
    Evidence:
    - Doc: ../../13-adrs/04-event-broker.md
    - Status: Deferred (interim default: NATS JetStream; revisit gate: Phase 2 or platform mandate)
    - Interim default documented in Defer Note section

[x] ADR-05: Observability Baseline (OpenTelemetry)
    Evidence:
    - Doc: ../../13-adrs/05-observability-baseline.md
    - Status: Accepted (updated from Proposed)

[x] ADR-06: Rule Pack Versioning
    Evidence:
    - Doc: ../../13-adrs/06-rule-pack-versioning.md
    - Status: Accepted (updated from Proposed)

[x] ADR-07: Approval Scope & Policy Snapshot Canonicalization
    Evidence:
    - Doc: ../../13-adrs/07-approval-scope-canonicalization.md
    - Status: Accepted (updated from Proposed)
```

---

## 5. Architecture Baseline

```
[x] System overview documented
    Evidence:
    - PR merged: #1
    - Doc: ../../02-architecture/01-system-overview.md — exists in docs baseline

[x] Component boundaries defined
    Evidence:
    - PR merged: #1
    - Doc: ../../02-architecture/02-components.md — exists in docs baseline

[x] Trust boundaries identified
    Evidence:
    - PR merged: #1
    - Doc: ../../02-architecture/03-trust-boundaries.md — exists in docs baseline

[x] Scaling topology defined
    Evidence:
    - PR merged: #1
    - Doc: ../../02-architecture/04-scaling-topology.md — exists in docs baseline

[x] Deployment models documented
    Evidence:
    - PR merged: #1
    - Doc: ../../02-architecture/05-deployment-models.md — exists in docs baseline
```

---

## 6. Local Dev Environment

```
[x] Rust toolchain 1.75+ configured
    Evidence:
    - File: rust-toolchain.toml (channel = "stable"; satisfies 1.75+ requirement)

[x] All workspace crates compile without errors
    Evidence:
    - `cargo check --workspace` passes (verified)

[ ] Postgres connection via Docker Compose working
    Evidence:
    - docker-compose.yml present with postgres service
    - Note: Not verified interactively — requires docker daemon

[ ] NATS local instance running
    Evidence:
    - docker-compose.yml present with nats service
    - Note: Not verified interactively — requires docker daemon

[ ] S3 mock (LocalStack or MinIO) configured
    Evidence:
    - docker-compose.yml present with minio service
    - Note: Not verified interactively — requires docker daemon

[x] Environment variables documented in .env.example
    Evidence:
    - File: .env.example
    - Variables: DATABASE_URL, NATS_URL, AWS_*, S3_*, OTEL_*, RUST_LOG
```

---

## 7. CI Baseline

```
[x] GitHub Actions (or equivalent) pipeline configured
    Evidence:
    - File: .github/workflows/ci.yml
    - Jobs: fmt, clippy, check, test, build

[x] CI runs: lint, check, test, build
    Evidence:
    - CI workflow defined with all four job types

[ ] Docker image build step present
    Evidence:
    - NOT PRESENT — docker build deferred to Phase 1 (no Dockerfile yet)

[ ] PR gate: all checks must pass before merge
    Evidence:
    - Branch protection not configured (infrastructure concern, not code)
```

---

## 8. Security Baseline

```
[x] Threat model v1 documented
    Evidence:
    - PR merged: #1
    - Doc: ../../08-security/01-threat-model.md — exists in docs baseline

[x] Authn/authz approach defined
    Evidence:
    - PR merged: #1
    - Doc: ../../08-security/02-authn-authz.md — exists in docs baseline

[x] Privacy and data handling policy defined
    Evidence:
    - PR merged: #1
    - Doc: ../../08-security/03-privacy-and-data-handling.md — exists in docs baseline

[x] Audit requirements defined
    Evidence:
    - PR merged: #1
    - Doc: ../../08-security/04-audit-and-compliance.md — exists in docs baseline
```

---

## 9. Operations Baseline

```
[x] Environments defined (dev, staging, prod)
    Evidence:
    - PR merged: #1
    - Doc: ../../09-operations/01-environments.md — exists in docs baseline

[x] CI/CD pipeline documented
    Evidence:
    - PR merged: #1
    - Doc: ../../09-operations/02-ci-cd.md — exists in docs baseline

[x] Observability plan defined
    Evidence:
    - PR merged: #1
    - Doc: ../../09-operations/03-observability.md — exists in docs baseline

[x] SLOs defined
    Evidence:
    - PR merged: #1
    - Doc: ../../09-operations/04-sre-and-slos.md — exists in docs baseline

[x] Runbooks skeleton created
    Evidence:
    - PR merged: #1
    - Doc: ../../09-operations/05-runbooks.md — exists in docs baseline
```

---

## 10. Agent Implementation Guide

```
[x] Agent implementation guide finalized
    Evidence:
    - PR merged: #1
    - Doc: ../../12-agents/01-agent-implementation-guide.md — exists in docs baseline

[x] Task breakdown defined
    Evidence:
    - PR merged: #1
    - Doc: ../../12-agents/02-task-breakdown.md — exists in docs baseline

[x] Prompts and contracts defined
    Evidence:
    - PR merged: #1
    - Doc: ../../12-agents/03-prompts-contracts.md — exists in docs baseline
```

---

## Exit Gate Confirmation

```
ALL ITEMS COMPLETE: □ Yes □ No

Exit Gate Review Date: ___________
Reviewed By: ___________

Blocking Issues (if any):
1. DB migration tooling (Section 3) — deferred to Phase 1
2. Docker image build (Section 7) — deferred to Phase 1
3. Branch protection rules (Section 7) — infrastructure concern
4. Postgres/NATS/S3 local verification — requires docker daemon (Sections 6–7)
```

**Next Phase:** [Phase 1 — Core Control Plane MVP](./checklist-phase-1.md)
