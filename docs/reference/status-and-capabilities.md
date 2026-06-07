# Status & Capabilities

> **Safety:** IRE is **not production-ready**. It is a bounded personal
> project intended for local development, integration experimentation,
> and study of the design. **Do not use it for production, sensitive, or
> customer-facing workloads** without independent validation.

This page is the **concise public status entry point**. It tells you, in
one read, what IRE is, what its current capabilities cover, and the
boundaries of safe use. For the per-capability breakdown, see the
[Capability Support Matrix](../01-product/04-capability-support-matrix.md).

---

## One-paragraph status

IRE is a Cargo workspace (Rust, stable, edition 2021) implementing a
**control-plane layer for intent change in agent workflows**. It is
delivered as a bounded personal project, with **local verification** as
the primary source of truth. IRE is **not production-ready** and is not
validated for production, sensitive, or customer-facing workloads.

---

## What IRE does

1. Normalizes an intent into a versioned, validated structure.
2. Computes a **semantic diff** between intent versions.
3. Builds a **dependency graph** between the intent and its artifacts,
   executions, and side effects.
4. Classifies impact (invalidations, reviews required, compensations) and
   proposes a **repair / rebased execution plan**.
5. Records **provenance** so each output can be traced back to the intent
   version that produced it.

For the formal positioning and scope boundaries, see
[Agent Safety Rebase](../01-product/03-agent-safety-rebase-positioning.md).
For the design rationale and the external patterns this draws on, see
[Rationale and external patterns](../99-reference/01-rationale-and-external-patterns.md).

---

## Capability support

The [Capability Support Matrix](../01-product/04-capability-support-matrix.md)
is the **per-capability source of truth**. The high-level shape today:

- **Bounded delivered:** intent versioning and semantic diff;
  dependency graph CRUD; rebase preview and apply; side-effect ledger and
  capture-on-write; compensation action CRUD and bounded executors; batch
  orchestration; policy gate evaluation; orchestration dashboard and
  dry-run; single-shot orchestration runtime; compensation simulation;
  forensic bundle verification, generation, and download; tenant isolation
  and RLS wiring (bounded); NATS consumer lifecycle and DLQ metrics
  (bounded).
- **Partial / bounded only:** full authentication / authorization
  (JWT + RLS bounded; full authz deferred); production observability, DLQ,
  webhook, and backup guarantees.
- **Out of scope for this project:** external SRE sign-off, external
  security sign-off, production-scale load testing, penetration testing,
  production infrastructure provisioning, full runtime replay, S3 Object
  Lock / immutable storage, full enforcement of RLS-based tenant isolation.

For the canonical, per-row, per-capability breakdown, see the
[Capability Support Matrix](../01-product/04-capability-support-matrix.md).

---

## CI is **not** a green-build guarantee

The GitHub Actions smoke workflow runs the same four checks as
`scripts/verify-fast.sh`; it is intentionally narrow. **Do not interpret
a green smoke run as production readiness**. For the local loop and the
smoke-vs-heavy-vs-manual split, see
[Development & Verification](../getting-started/development.md).

---

## Honest usage guidance

If you are evaluating IRE for any sensitive, customer-facing, or
production workload, **do not** rely on it. Use it only for local
development, integration experimentation, and bounded study of the
design.

For the policy on reporting issues, see
[`CONTRIBUTING.md`](../../CONTRIBUTING.md) and the
[.github/ISSUE_TEMPLATE](../../.github/ISSUE_TEMPLATE/) directory. For
security disclosures, see [`SECURITY.md`](../../SECURITY.md). For support
expectations, see
[.github/SUPPORT.md](../../.github/SUPPORT.md) (there is **no SLA**).
