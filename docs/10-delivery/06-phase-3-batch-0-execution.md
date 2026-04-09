# Phase 3 Batch 0 — Execution Plan

## Goal

Complete the remaining Phase 3 Batch 0 preparation work without overclaiming Batch 1+ implementation.

Batch 0 is limited to:
- planning and sequencing,
- package scaffolds,
- dependency audit groundwork,
- threat-model and SLO preparation inputs.

It does **not** include production side-effect capture, compensation execution, forensic generation, S3/NATS integration, or observability rollout.

---

## Current State

### Completed in the scaffold slice

1. Added workspace crates:
   - `crates/compensation-service/`
   - `crates/forensic-service/`
2. Added spec-backed domain scaffolds:
   - `SideEffect`, `SideEffectClass`
   - `CompensationAction`, `CompensationFeasibility`, `StrategyType`, `CompensationStatus`
   - `ForensicBundle`, `BundleIntegrity`, `BundleTimeRange`, `BundlePurpose`, `BundleContents`
3. Extended audit taxonomy groundwork for future Phase 3 flows:
   - compensation lifecycle events
   - forensic bundle request/generation events
4. Verified scaffold crates with focused tests and workspace build.

### Remaining Batch 0 items

1. Record section ownership and sign-off placeholders for Sections 1–7.
2. Write a dependency audit for cross-service assumptions introduced by Phase 2b.
3. Confirm or restate provisional SLO targets for later SRE sign-off.
4. Gather Phase 2b security findings as inputs for Threat Model v2.

---

## Execution Order

### Slice B0-A — Delivered

- Add compensation/forensic workspace scaffolds
- Add type-level domain groundwork
- Add additive audit taxonomy groundwork
- Verify with `cargo fmt`, `cargo check`, and focused crate tests

### Slice B0-B — Dependency Audit + Phase 2b Gap Review

Deliverables:
- one dependency audit doc covering:
  - event subject/version assumptions,
  - DLQ and consumer-group assumptions,
  - S3 path assumptions for artifacts/quarantine/forensic bundles,
  - DB schema assumptions for side effects, rollbacks, and optimization indexes,
  - missing service boundaries currently implied by docs.

Recommended output:
- `docs/10-delivery/07-phase-3-dependency-audit.md`

### Slice B0-C — Ownership + SLO + Security Inputs

Deliverables:
- owner/sign-off placeholders captured in docs,
- provisional SLO targets aligned to existing operations docs,
- summarized Phase 2b findings to seed Threat Model v2 and residual risk work.

Recommended outputs:
- update `docs/10-delivery/05-phase-3-hardening.md`
- update `docs/09-operations/04-sre-and-slos.md` if targets need normalization
- add a short Phase 2b-to-Phase 3 security findings note if needed

---

## Batch 0 Success Criteria

Batch 0 can be marked complete only when all of the following are true:

- scaffold crates exist and build,
- dependency audit artifact exists,
- section ownership/sign-off placeholders are recorded,
- provisional SLO targets are documented,
- Phase 2b findings are captured as Threat Model v2 input,
- docs clearly distinguish Batch 0 groundwork from Batch 1+ delivery.

---

## Verification

For the delivered scaffold slice, verification is:

- `cargo fmt --all`
- `cargo check --workspace`
- `cargo test -p compensation-service --all-features`
- `cargo test -p forensic-service --all-features`

For the remaining docs-only slices, verification is manual consistency across:

- `docs/10-delivery/checklists/checklist-phase-3.md`
- `docs/10-delivery/05-phase-3-hardening.md`
- any new dependency-audit or security-input artifact.
