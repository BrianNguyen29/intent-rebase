# 13 — Reviewer Guide for DuongNguyen

**Status:** `PREPARATION — Review Pending; No Sign-Off Claimed`
**Phase:** Phase 4 Entry
**Owner:** Backend Lead (solo practitioner)
**Reviewer Designation:** DuongNguyen
**Last Updated:** 2026-06-15

---

## Purpose

This guide prepares DuongNguyen to perform a real review of the A-03 (SRE Operational Readiness) and A-04 (Security Architecture) gates. It maps each review item to the existing evidence documents and commands so the reviewer can verify claims independently without re-deriving the inventory.

> **⚠️ Non-Claim Disclaimer**
>
> This document is a **review preparation packet**, not a sign-off. No approval boxes are ticked. No external gates are closed. Production readiness is **not** claimed. The reviewer must independently assess evidence and record findings in the existing external review packet (`docs/09-operations/10-external-review-packet.md`, Section H).

---

## Scope and Non-Claims

| What This Guide Does | What This Guide Does Not Do |
|----------------------|-----------------------------|
| Lists evidence files and commands for each A-03/A-04 item | Tick approval checkboxes or sign-off |
| Separates local evidence from blocked/external items | Claim production readiness or CI-green status |
| Points to the exact packet section where the reviewer records findings | Replace the external review packet (`10-external-review-packet.md`) |
| Suggests commands the reviewer can run to verify bounded local claims | Close any gate (A-05 through A-13) |

---

## Reviewer: DuongNguyen

**Role:** External reviewer (designated)
**Prerequisites Before Starting:**

1. Clone the repository at the commit hash under review (latest: `main` or tagged release).
2. Verify you can run the commands in §6 on your local environment (Rust toolchain, Docker Compose for optional services).
3. Read `docs/09-operations/10-external-review-packet.md` Sections A–D before Sections E–F.

**Review Boundaries:**
- You are **not** expected to verify production infrastructure (A-05), staging load tests (A-06 L3–L5), pen test execution (A-07), or production-grade NATS/DLQ (A-10). Those are explicitly blocked and tracked separately.
- You **are** expected to assess whether the local evidence is sufficient to justify moving to the next phase (staging), and to identify gaps that must close before production.

---

## A-03 SRE Review Checklist

### 1. SLO Definitions and Alerting Rules

| Review Item | Evidence Location | What to Verify | Reviewer Action |
|-------------|-------------------|----------------|-----------------|
| SLO targets are documented | `docs/09-operations/04-sre-and-slos.md` | Read SLO table (availability, latency, error budget). | Assess whether targets are realistic for the architecture. Record assessment in `10-external-review-packet.md` Section E.1. |
| Alert rules are documented | `docs/09-operations/04-sre-and-slos.md` + `docs/09-operations/12-panic-alerting-integration.md` | Read alert thresholds, severity levels, and intended routing. | Assess whether thresholds and severities are appropriate. Record in `10-external-review-packet.md` Section E.2. |
| Alert rule syntax is valid | `docs/09-operations/12-panic-alerting-integration.md` §4.2 | Run `promtool check rules <rule-file>` if a design-only rule file exists. | Verify syntax passes. Note: real Alertmanager receivers are not configured locally. |
| Observability stack is locally wired | `docs/09-operations/09-observability-evidence-checklist.md` | Check that metrics endpoint, Prometheus scrape, and Grafana dashboards are documented. | Verify the checklist is complete. Local evidence only — production telemetry is not connected. |

### 2. Runbooks

| Review Item | Evidence Location | What to Verify | Reviewer Action |
|-------------|-------------------|----------------|-----------------|
| Runbooks exist for all P1 alerts | `docs/09-operations/05-runbooks.md` (RB1–RB15) | Read each runbook for completeness, escalation path, and caveats. | Assess whether runbooks are actionable and complete. Record in `10-external-review-packet.md` Section E.3. |
| Panic-specific runbook exists | `docs/09-operations/05-runbooks.md` (RB15) | Read RB15 "Process Panic Detected". | Assess whether the panic response procedure is clear. |

### 3. Backup / Restore

| Review Item | Evidence Location | What to Verify | Reviewer Action |
|-------------|-------------------|----------------|-----------------|
| Backup procedures are documented | `docs/09-operations/07-backup-restore.md` | Read RPO/RTO targets and procedures. | Assess whether RPO=1h / RTO=30m is achievable. Record in `10-external-review-packet.md` Section E.4. |
| Local restore test evidence exists | `docs/09-operations/07-backup-restore.md` §Execution Log | Verify a `pg_dump`/`pg_restore` was executed against a local docker-compose Postgres. | Confirm the log shows a successful restore and a post-restore test pass. This is local evidence only. |

### 4. Local Commands Reviewer May Run

See §6 for the full command list. Key SRE verification commands:

```bash
# Verify local canonical gates pass (non-production, no external services)
cargo fmt --all -- --check
cargo check --workspace --all-features
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --lib --all-features

# Verify Prometheus rule syntax (if rule file is present)
# promtool check rules docs/09-operations/rules/*.yaml

# Verify docker-compose stack comes up (optional local infra)
docker compose -f infrastructure/local/docker-compose.yml up -d
```

---

## A-04 Security Review Checklist

### 1. Authentication & Authorization

| Review Item | Evidence Location | What to Verify | Reviewer Action |
|-------------|-------------------|----------------|-----------------|
| JWT auth implementation | `docs/08-security/02-authn-authz.md` | Read JWT guard, RLS context helper, and token validation. | Assess whether JWT validation is sufficient. Record in `10-external-review-packet.md` Section F.1. |
| RLS tenant isolation | `docs/08-security/02-authn-authz.md` + `docs/11-quality/03-rls-audit.md` | Read RLS wrapping status and audit script output. | Assess whether tenant isolation is properly enforced at the SQL layer. Note: full transaction wrapping is bounded partial. |
| API key scaffold | `docs/08-security/02-authn-authz.md` | Read per-tenant API key design. | Assess whether key rotation and revocation are addressed. |

### 2. Threat Model

| Review Item | Evidence Location | What to Verify | Reviewer Action |
|-------------|-------------------|----------------|-----------------|
| Threat model v2 exists | `docs/14-governance/06-threat-model-v2.md` | Read threat taxonomy, attack scenarios, and mitigations. | Assess whether threats are properly identified and mitigations are sufficient. Record in `10-external-review-packet.md` Section F.3. |
| Pen test scope exists | `docs/08-security/06-pen-test-scope.md` | Read scope definition, in-scope/out-of-scope items, and test types. | Assess whether the scope is appropriate. Record in `10-external-review-packet.md` Section F.4. |

### 3. Data Protection & Audit

| Review Item | Evidence Location | What to Verify | Reviewer Action |
|-------------|-------------------|----------------|-----------------|
| Audit event schema | `docs/14-governance/01-audit-event-spec.md` | Read canonical schema, event taxonomy, and integrity design. | Assess whether the schema supports compliance and forensic requirements. Record in `10-external-review-packet.md` Section F.2. |
| Audit implementation status | `docs/14-governance/01-audit-event-spec.md` §Current bounded implementation status | Read bounded implementation notes (Phase 2b). | Assess whether the bounded implementation is sufficient for staging. Note: append-only enforcement, hash-chain verification, and NATS/S3 integration remain target-state. |
| Secrets inventory | `docs/09-operations/08-secrets-inventory.md` | Read inventory list and rotation procedures. | Assess whether secret management is adequate. Note: Vault/AWS SM are not deployed. |

### 4. Local Commands Reviewer May Run

See §6 for the full command list. Key security verification commands:

```bash
# Verify JWT-auth tests pass (feature-gated)
cargo test -p intent-api --lib auth --features jwt-auth

# Verify RLS audit script runs (structural review, no live DB required)
# bash scripts/audit-rls-dml.sh

# Verify public repo secret scan (no high-confidence secrets found)
# grep -r "password\|secret\|token\|api_key" crates/ --include="*.rs" | grep -v "test\|mock\|example"
```

---

## Items Still Blocked / Not Review-Complete

The following items are **out of scope** for this review because they require external infrastructure, teams, or future phases. Do not assess them as part of A-03/A-04. They are tracked separately and remain blocked.

| Gate | Why Blocked | What Would Unblock It |
|------|-------------|---------------------|
| A-05 Production Infrastructure | No production environment provisioned | Cloud provider account, Terraform/CDK, deployment runbook execution |
| A-06 Load Testing L3–L5 | Staging/production infrastructure required | Staging environment, k6/Artillery harness, 30min sustained load evidence |
| A-07 Penetration Testing | No external pen test team engaged | External pen test engagement, staging environment, report with HIGH/CRITICAL remediation |
| A-10 DLQ/NATS Production-Grade | Full replay worker and production NATS topology deferred | External SRE sign-off, production JetStream config, fault-injection validation |
| A-11 Cross-Process Trace Propagation | Temporal SDK lacks safe per-request gRPC metadata injection | Temporal SDK fix, sqlx per-query context feature, NATS publisher implementation |
| A-12 Webhook Delivery Production Hardening | Secret manager, key rotation, staging evidence missing | Vault/AWS SM, real subscriber endpoint, delivery SLO evidence, external review |
| A-13 Forensic Replay + Immutable Storage | S3 Object Lock not deployed, chain-hash not implemented | S3 Object Lock infrastructure, chain-hash implementation, retention enforcement |

---

## How to Record Completion

When the review is complete, record findings in the following existing documents **only**. Do not create new evidence documents for the review outcome.

1. **Primary record:** `docs/09-operations/10-external-review-packet.md`
   - Section E (SRE): Fill reviewer assessment fields for each table.
   - Section F (Security): Fill reviewer assessment fields for each table.
   - Section G (Findings Tracker): Add rows for any findings (FIND-001, FIND-002, ...).
   - Section H (Sign-Off): Check the appropriate boxes **only after** you have verified the evidence independently. Do not pre-check boxes.

2. **Tracker update:** `docs/10-delivery/23-project-assessment-and-execution-tracker.md`
   - Add one concise update-log row under §13 noting the review date, reviewer name, and high-level outcome (e.g., "DuongNguyen completed A-03/A-04 review; findings recorded in 10-external-review-packet.md Section G; Section H sign-off pending remediation of FIND-001, FIND-002").
   - Do not change the status of any blocked gate (A-05 through A-13) to complete.

3. **Phase 4 plan update:** `docs/10-delivery/22-phase-4-entry-plan.md`
   - Add one concise update-log row under the Update Log (§Update Log) referencing the review and the packet.
   - Update the A-03 and A-04 status lines **only** to reflect that the review was conducted and findings exist. Do not change status to "APPROVED" or "COMPLETE" unless Section H is fully signed.

**Self-signing prohibition:** BrianNguyen (the solo practitioner) is not authorized to sign Section H on behalf of DuongNguyen. Section H must be completed by the designated reviewer only.

---

## Local Commands Reviewer May Run

The following commands produce local evidence. They do **not** require production infrastructure. They are the same commands used by the solo practitioner for bounded local verification.

```bash
# Format and type checks
cargo fmt --all -- --check
cargo check --workspace --all-features
cargo clippy --workspace --all-targets -- -D warnings

# Unit tests (in-memory, no external services)
cargo test --workspace --lib --all-features

# Targeted crate tests (faster than workspace)
cargo test -p intent-api --lib
cargo test -p intent-service --lib
cargo test -p graph-service --lib
cargo test -p rebase-engine --lib
cargo test -p rebase-orchestrator --lib
cargo test -p forensic-service --lib
cargo test -p compensation-service --lib

# RLS integration (requires live docker-compose Postgres + DATABASE_URL)
# DATABASE_URL=postgres://intent_rebase:intent_rebase_dev@localhost:5432/intent_rebase_phase1_fix \
#   cargo test -p intent-api --test rls_integration -- --ignored

# NATS JetStream live integration (requires live NATS + NATS_URL)
# NATS_URL=nats://localhost:4222 \
#   cargo test -p intent-api --lib nats_jetstream -- --ignored

# Webhook integration (requires live docker-compose Postgres + DATABASE_URL)
# DATABASE_URL=postgres://intent_rebase:intent_rebase_dev@localhost:5432/intent_rebase_phase1_fix \
#   cargo test -p intent-api --test webhook_integration -- --ignored

# Diff hygiene
git diff --check
```

> **Note:** Commands prefixed with `#` require external services (docker-compose stack) and are optional. They are documented for completeness but are not required for the A-03/A-04 review.

---

## Forbidden Claims

| Forbidden Claim | Allowed Replacement |
|----------------|-------------------|
| `DuongNguyen approved A-03/A-04` | `DuongNguyen conducted review; findings recorded in 10-external-review-packet.md; sign-off pending` |
| `External SRE sign-off obtained` | `External SRE review packet template exists; sign-off pending external review` |
| `External security review complete` | `Security review packet template exists; review pending external engagement` |
| `Production-ready per reviewer` | `Reviewer guide prepared; production readiness pending reviewer findings and sign-off` |

---

## Relationship to Other Documents

| Document | Relationship |
|----------|--------------|
| `docs/09-operations/10-external-review-packet.md` | Primary packet where reviewer records findings and sign-off (Section H) |
| `docs/10-delivery/22-phase-4-entry-plan.md` | Phase 4 entry plan with A-03/A-04 status and external gate tracker |
| `docs/10-delivery/23-project-assessment-and-execution-tracker.md` | Consolidated tracker for update-log entries |
| `docs/09-operations/04-sre-and-slos.md` | SLO definitions and alert rules under review |
| `docs/08-security/02-authn-authz.md` | Authn/authz implementation under review |
| `docs/14-governance/06-threat-model-v2.md` | Threat model under review |
| `docs/08-security/06-pen-test-scope.md` | Pen test scope under review |
| `docs/09-operations/05-runbooks.md` | Runbooks under review |
| `docs/09-operations/07-backup-restore.md` | Backup/restore procedures under review |
| `docs/09-operations/08-secrets-inventory.md` | Secrets management under review |
| `docs/09-operations/09-observability-evidence-checklist.md` | Observability local evidence under review |
| `docs/09-operations/12-panic-alerting-integration.md` | Panic alerting design and local metric evidence under review |
| `docs/14-governance/01-audit-event-spec.md` | Audit schema and bounded implementation under review |

---

## Update Log

| Date | Updated By | Changes |
|------|------------|---------|
| 2026-06-15 | BrianNguyen (via authorized assistant fixer) | Initial reviewer guide created for DuongNguyen. Maps A-03 SRE and A-04 Security review items to evidence files, commands, and the external review packet recording workflow. Explicitly separates blocked items (A-05 through A-13) from review scope. No sign-off claimed. No gates closed. No production-readiness claim. |
