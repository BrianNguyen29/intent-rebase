# Phase 0 — Foundations Checklist

**Exit Gate:** Phase 0 complete khi tất cả items bên dưới checked và có evidence.

**Trạng thái:** `✓ Complete`  
**Phase:** Phase 0  
**Target Duration:** 2–4 tuần

---

## 1. Product Spec Finalized

```
[ ] Product thesis finalized
    Evidence:
    - PR merged: <link>
    - Doc: ../../01-product/01-product-thesis.md

[ ] Goals and non-goals defined
    Evidence:
    - Doc: ../../01-product/02-goals-nongoals.md

[ ] Core principles documented
    Evidence:
    - Doc: ../../01-product/03-core-principles.md

[ ] Use cases documented
    Evidence:
    - Doc: ../../01-product/04-use-cases.md

[ ] Glossary updated
    Evidence:
    - Doc: ../../01-product/05-glossary.md
```

---

## 2. Domain Model Finalized

```
[ ] Intent model v1 defined
    Evidence:
    - Doc: ../../03-spec/01-intent-model.md

[ ] Semantic diff approach documented
    Evidence:
    - Doc: ../../03-spec/02-semantic-diff.md

[ ] Dependency graph model documented
    Evidence:
    - Doc: ../../03-spec/03-dependency-graph.md

[ ] Rebase algorithm documented
    Evidence:
    - Doc: ../../03-spec/04-rebase-engine.md

[ ] Compensation model documented
    Evidence:
    - Doc: ../../03-spec/05-compensation.md

[ ] Provenance model documented
    Evidence:
    - Doc: ../../03-spec/06-provenance.md
```

---

## 3. Repository Scaffolding

```
[ ] Rust workspace created
    Evidence:
    - PR merged: <link>
    - Cargo.toml with workspace members

[ ] Service structure defined (intent-service, rebase-engine, graph-service)
    Evidence:
    - Directory structure in repo

[ ] Shared crate created (intent-rebase-types)
    Evidence:
    - crate published to local registry or workspace member

[ ] Database migration tool integrated (sqlx-cli or similar)
    Evidence:
    - Migrations runnable locally

[ ] Docker Compose for local infra (Postgres, NATS)
    Evidence:
    - docker-compose.yml present and working
```

---

## 4. ADRs Accepted

```
[ ] ADR-01: Runtime Adapter Selection (Temporal)
    Evidence:
    - Doc: ../../13-adrs/01-runtime-adapter.md
    - Status: Accepted

[ ] ADR-02: Data Plane Architecture (Postgres + S3)
    Evidence:
    - Doc: ../../13-adrs/02-data-plane.md
    - Status: Accepted

[ ] ADR-03: External API Protocol (REST + Webhooks)
    Evidence:
    - Doc: ../../13-adrs/03-external-api.md
    - Status: Accepted

[ ] ADR-04: Event Broker Selection (NATS or Kafka)
    Evidence:
    - Doc: ../../13-adrs/04-event-broker.md
    - Status: Accepted

[ ] ADR-05: Observability Baseline (OpenTelemetry)
    Evidence:
    - Doc: ../../13-adrs/05-observability-baseline.md
    - Status: Accepted

[ ] ADR-06: Rule Pack Versioning
    Evidence:
    - Doc: ../../13-adrs/06-rule-pack-versioning.md
    - Status: Accepted

[ ] ADR-07: Approval Scope & Policy Snapshot Canonicalization
    Evidence:
    - Doc: ../../13-adrs/07-approval-scope-canonicalization.md
    - Status: Accepted
```

---

## 5. Architecture Baseline

```
[ ] System overview documented
    Evidence:
    - Doc: ../../02-architecture/01-system-overview.md

[ ] Component boundaries defined
    Evidence:
    - Doc: ../../02-architecture/02-components.md

[ ] Trust boundaries identified
    Evidence:
    - Doc: ../../02-architecture/03-trust-boundaries.md

[ ] Scaling topology defined
    Evidence:
    - Doc: ../../02-architecture/04-scaling-topology.md

[ ] Deployment models documented
    Evidence:
    - Doc: ../../02-architecture/05-deployment-models.md
```

---

## 6. Local Dev Environment

```
[ ] Rust toolchain 1.75+ configured
    Evidence:
    - rust-toolchain.toml present

[ ] All workspace crates compile without errors
    Evidence:
    - `cargo check --all` passes

[ ] Postgres connection via Docker Compose working
    Evidence:
    - `cargo test` with DB integration tests pass

[ ] NATS local instance running
    Evidence:
    - `nats-server` starts and accepts connections

[ ] S3 mock (LocalStack or MinIO) configured
    Evidence:
    - S3 operations work locally

[ ] Environment variables documented in .env.example
    Evidence:
    - .env.example present with all required vars
```

---

## 7. CI Baseline

```
[ ] GitHub Actions (or equivalent) pipeline configured
    Evidence:
    - .github/workflows/ci.yml present

[ ] CI runs: lint, check, test, build
    Evidence:
    - CI green on main branch

[ ] Docker image build step present
    Evidence:
    - Docker build succeeds in CI

[ ] PR gate: all checks must pass before merge
    Evidence:
    - Branch protection rules configured
```

---

## 8. Security Baseline

```
[ ] Threat model v1 documented
    Evidence:
    - Doc: ../../08-security/01-threat-model.md

[ ] Authn/authz approach defined
    Evidence:
    - Doc: ../../08-security/02-authn-authz.md

[ ] Privacy and data handling policy defined
    Evidence:
    - Doc: ../../08-security/03-privacy-and-data-handling.md

[ ] Audit requirements defined
    Evidence:
    - Doc: ../../08-security/04-audit-and-compliance.md
```

---

## 9. Operations Baseline

```
[ ] Environments defined (dev, staging, prod)
    Evidence:
    - Doc: ../../09-operations/01-environments.md

[ ] CI/CD pipeline documented
    Evidence:
    - Doc: ../../09-operations/02-ci-cd.md

[ ] Observability plan defined
    Evidence:
    - Doc: ../../09-operations/03-observability.md

[ ] SLOs defined
    Evidence:
    - Doc: ../../09-operations/04-sre-and-slos.md

[ ] Runbooks skeleton created
    Evidence:
    - Doc: ../../09-operations/05-runbooks.md
```

---

## 10. Agent Implementation Guide

```
[ ] Agent implementation guide finalized
    Evidence:
    - Doc: ../../12-agents/01-agent-implementation-guide.md

[ ] Task breakdown defined
    Evidence:
    - Doc: ../../12-agents/02-task-breakdown.md

[ ] Prompts and contracts defined
    Evidence:
    - Doc: ../../12-agents/03-prompts-contracts.md
```

---

## Exit Gate Confirmation

```
ALL ITEMS COMPLETE: □ Yes □ No

Exit Gate Review Date: ___________
Reviewed By: ___________

Blocking Issues (if any):
1.
2.
3.
```

**Next Phase:** [Phase 1 — Core Control Plane MVP](./checklist-phase-1.md)