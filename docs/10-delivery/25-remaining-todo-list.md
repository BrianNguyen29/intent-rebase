# Remaining Todo List (Internal)

> **Status:** INTERNAL PLANNING — canonical remaining todo-list from verified audit
> **Date:** 2026-06-10
> **Owner:** BrianNguyen (Backend Lead, solo practitioner)
> **Scope:** Separates completed work, local-verifiable remaining tasks, risky/design-first work, and external-gated blockers. No production-readiness, CI-green, or external-signoff claims.
> **Non-Production Caveat:** This document is an internal planning artifact. It does not constitute production-readiness evidence, it does not claim CI-green status, and it does not claim external sign-off. All external and production evidence gates remain blocked or deferred.

---

## 1. Completed Work (Reference Only)

The following items are **already done** and are listed here only to avoid re-auditing. Evidence lives in the cited documents.

| Item | Evidence | Notes |
|------|----------|-------|
| Phase 0–3 non-production delivery | `00-current-status.md`, `23-project-assessment-and-execution-tracker.md` §3, §5, §6, §7 | Local-dev bounded slices only; no production claim |
| Strategic 24b–24j items | `24-strategic-roadmap-and-checklist.md` §3, §4–§9 | Runtime adapter E2E proof, API decomposition demo, webhook SQL wiring, CLI decoupling, benchmark compile guard, language policy acceptance, survey, Tier 1 + Tier 2 migration |
| AGENTS fix | `AGENTS.md` | Internal agent rules updated |
| Tier 1 / Tier 2 language migration | `24i-runtime-adapter-adr-language-migration-evidence.md`, `24j-documentation-language-tier1-tier2-evidence.md` | `docs/13-adrs/01-runtime-adapter.md`, `docs/10-delivery/01-roadmap.md`, `docs/10-delivery/04-phase-2-runtime-integrated.md` translated; 15 header-only files swept (`## Mục đích` → `## Purpose`) |
| Checklist cleanup | `23-project-assessment-and-execution-tracker.md` §13 (2026-06-08) | Five stale duplicate root-level checklist stubs removed; canonical checklists under `checklists/` untouched |
| Local verification refresh | `23-project-assessment-and-execution-tracker.md` §3.3, §4.1 | fmt/check/clippy/lib tests pass; fresh DB ignored suites green (migration 2/2, RLS 22/22, NATS 14/14, SQLx smoke 7/7, webhook 1/1) |
| OpenAPI spectral error-level pass | `23-project-assessment-and-execution-tracker.md` §9 | `npx @stoplight/spectral-cli lint` passes with `--fail-severity=error` |

---

## 2. Remaining Local-Executable Tasks

These tasks can be executed locally without external reviewers, production infrastructure, or vendor engagement. They are ordered roughly by bounded-slice effort (smallest first).

### A. Tier 3 Full Translations

**What:** Translate 34 full-Vietnamese internal docs to English per the accepted Documentation Language Policy (`24g` §3, `24h` §5.2).

**Files:** `docs/01-product/` (5), `docs/02-architecture/` (5), `docs/03-spec/` (6), `docs/04-api/` (3), `docs/05-data/` (2), `docs/06-backend/` (4), `docs/07-frontend/` (1), `docs/08-security/` (4), `docs/11-quality/` (1), `docs/12-agents/` (2), `docs/99-reference/` (1). 34 files enumerated in the current inventory; full list in `24h` §5.2.

**Approach:** Bounded single-file or small-batch slices; each slice must run the public-doc leakage scan, affirmative-claim scan, and `git diff --check` per `24-strategic-roadmap-and-checklist.md` §9.7.

**Status:** ⬜ Not started.

---

### B. ADR / Governance README Translations + Minor Single-Word Fix

**What:** Translate 2 remaining ADR/governance README files to English, plus one minor single-word fix in an ops doc.

**Files:**
- `docs/13-adrs/README.md` — ADR index README
- `docs/14-governance/README.md` — Governance index README
- `docs/09-operations/03-observability.md` line 28 — replace `với` with English equivalent (e.g., `with`)

**Correction:** The `24h` §5.3 inventory lists 3 files, but `docs/13-adrs/01-runtime-adapter.md` was **already translated** in `24i` / `24j`. Do not re-count it as remaining.

**Status:** ✅ DONE.

---

### C. Mixed Delivery Docs

**What:** Translate or classify the remaining mixed English/Vietnamese delivery docs.

**Files (10 remaining, excluding the 2 Tier1 files already done):**
- `docs/10-delivery/02-phase-0-foundations.md`
- `docs/10-delivery/03-phase-1-core-control-plane.md`
- `docs/10-delivery/06-phase-4-expansion.md`
- `docs/10-delivery/11-phase-2b-sign-off-packet.md`
- `docs/10-delivery/checklists/README.md`
- `docs/10-delivery/checklists/checklist-phase-0.md`
- `docs/10-delivery/checklists/checklist-phase-1.md`
- `docs/10-delivery/checklists/checklist-phase-2.md`
- `docs/10-delivery/checklists/checklist-phase-3.md`
- `docs/10-delivery/checklists/checklist-phase-4.md`

**Note:** `docs/10-delivery/01-roadmap.md` and `docs/10-delivery/04-phase-2-runtime-integrated.md` were completed in `24j`; do not list them as remaining.

**Status:** ✅ DONE.

---

### D. Code TODOs

**What:** Resolve or document two flagged code-level TODO items.

| Location | Line | Text | Action |
|----------|------|------|--------|
| `crates/intent-api/src/webhook_dispatcher.rs` | 79 | `TODO(Slice 4+): version info should be stored in the outbox record at creation time.` | **DEFERRED / DESIGN-FIRST** — requires schema decision and explicit approval before implementation. See evidence doc D1 rationale. |
| `crates/intent-rebase-types/src/graph.rs` | 488 | `Replaces the Phase 1 baseline TODO structure with graph-integrated classification.` | ✅ DONE — wording updated to "placeholder structure"; no functional change. |

**Status:** D1 deferred; D2 done.

---

### E. Stale Contradiction Cleanup

**What:** Re-verify and close residual contradictions flagged in the tracker.

| ID | Tracker Claim | Residual Risk | Next Action |
|----|---------------|---------------|-------------|
| **C-1** | P0-1 marked ✅ RESOLVED | `17-production-readiness-backlog.md` P2-6 vs `22-phase-4-entry-plan.md` A-12 status may have drifted since 2026-05-20 | Re-read both docs; update if any newly delivered local-dev slices are not reflected |
| **C-8** | P0-8 marked ✅ RESOLVED | `20-project-completion-roadmap.md` P2 "Docs Complete" vs Phase 4 decomposition may still lag | Verify roadmap accurately reflects delivered decomposition (A-09 S6, route groups, test extractions) |
| **C-9** | P0-9 marked ✅ RESOLVED | `10-external-review-packet.md` G-RLS-1 may lag behind latest RLS integration status | Sync G-RLS-1 with `22-phase-4-entry-plan.md` A-02 and `17-production-readiness-backlog.md` P1-S5i if any changes landed since 2026-05-20 |
| **A-09** | Tracker / 24-strategic note deferred cross-update | `22-phase-4-entry-plan.md` A-09 status line was explicitly deferred in `24e` line 89 and `24-strategic` §5.5 | ✅ DONE — continuation note added to A-09 status line tracking `health_routes` → `routes::health` demo slice and next candidate |

**Status:** A-09 done; C-8 wording polished.

---

### F. intent-cli Short Flag Audit

**What:** Audit all `#[arg(short)]` declarations in `crates/intent-cli/src/lib.rs` for latent collisions.

**Context:** The top-level `-a` collision between `api_url` and `api_key` was fixed in `24d` (`short = 'u'` and `short = 'k'`). Subcommands (`Run`, `GetRun`) use default short forms that may collide if new fields are added.

**Approach:** Run `cargo run -p intent-cli -- --help` and `cargo run -p intent-cli -- run --help` / `cargo run -p intent-cli -- get-run --help`; verify no `clap` panic or duplicate short flags.

**Status:** ✅ DONE — audit revealed latent `-i` collision between `intent_id` and `initiated_by` in `Run` subcommand; fixed by assigning `short = 'b'` to `initiated_by`. All three help commands now exit 0. Top-level `-a` (api_url/api_key) collision was already fixed in `24d` (`-u` / `-k`).

---

### G. Benchmark Manifest Symmetry for Auto-Discovered Benches

**What:** Add explicit `[[bench]]` stanzas in `Cargo.toml` for auto-discovered bench files to ensure manifest symmetry.

**Context:** `24f` observed that `intent-service/benches/query_latency.rs` and `rebase-engine/benches/diff_latency.rs` compile via Cargo auto-discovery but have no matching `[[bench]]` stanza in their respective `Cargo.toml` files. The other 5 bench files have explicit stanzas.

**Approach:** Add `[[bench]] name = "query_latency" harness = false` to `crates/intent-service/Cargo.toml` and `[[bench]] name = "diff_latency" harness = false` to `crates/rebase-engine/Cargo.toml`. Verify `cargo bench --workspace --no-run` still exits 0.

**Status:** ✅ DONE — explicit stanzas added; `cargo check --benches -p intent-service -p rebase-engine` passes (full `--no-run` timed out in local env but check confirms manifest symmetry).

---

## 3. Risky / Design-First Work

These items require design documents, explicit approval, or architectural review before implementation. They are **not** simple bounded slices.

| Item | Why Design-First | Current State | Blocker |
|------|------------------|---------------|---------|
| **NATS per-tenant JetStream streams** | Requires stream migration plan, tenant UUID inventory, rollback strategy, and SRE coordination | Stage 1 readiness assessment done (`23-project-assessment-and-execution-tracker.md` I4); shared `audit_events` stream still in use | External SRE sign-off (A-03), staging env (A-05) |
| **Webhook production delivery** | Requires secret manager integration, key rotation grace window, HMAC signing, delivery SLO, and real external receiver validation | Local-dev bounded slices delivered (`24c`, webhook integration test); SQLx outbox + subscription repos wired | External security review (A-04), production infra (A-05), SRE sign-off (A-03) |
| **S3/S4 side-effect auto-compensation** | Requires explicit approval per repo-specific change constraints (AGENTS.md constraint #5) before implementation | Design baseline exists; no implementation started | Explicit user approval required before any code change |
| **Forensic Object Lock / chain-hash** | Requires S3 Object Lock deployment, retention enforcement, tamper-evidence validation, and security review | Local-dev bundle generation/export/download delivered; S3-backed storage env-gated (`FORENSIC_BUNDLE_STORAGE=s3`) | Production infrastructure (A-05), external security review (A-04) |

> **No overclaim:** None of these items are in progress. They are tracked for roadmap completeness only.

---

## 4. External-Gated Blockers

These gates **cannot** be closed by local work. They require named independent third-party evidence, production infrastructure, or upstream SDK fixes.

| Gate | Required Evidence | Owner | Local Status |
|------|-------------------|-------|--------------|
| **A-03** | Named external SRE reviewer; signed Section H of external review packet | External SRE (to be named) | 🔴 Blocked |
| **A-04** | Named external security reviewer; threat model v2 assessment | External Security (to be named) | 🔴 Blocked |
| **A-05** | Production Postgres/NATS/S3/monitoring operational; deployment runbook executed | SRE | 🔴 Blocked |
| **A-06** | L4 30min sustained + all alert types + real receivers; L5 production load test | Backend Lead / SRE | 🔴 Blocked (I2a 10min + 1 alert sub-slice recorded only) |
| **A-07** | External pen test report (PDF + JSON); HIGH/CRITICAL remediation | External Pen Test Team | 🔴 Blocked |
| **A-10** | Production NATS topology; full DLQ replay worker validated; SRE sign-off | Backend Lead / SRE | 🔴 Blocked |
| **A-11** | Temporal SDK safe per-request gRPC metadata injection; cross-process trace IDs in OTLP | Backend Lead / SRE | 🔴 Deferred / SDK-blocked |
| **A-12** | Production secret manager + key rotation; staging/production SLO evidence; external review closure | SRE / Security | 🔴 Blocked |
| **A-13** | S3 Object Lock deployed; chain-hash; retention enforcement validated | Backend Lead / Security | 🔴 Blocked |

**Production SRE / security / infra / load / pen / observability / DLQ / webhook / backup gates:** All remain open. No solo self-review is sufficient to close any external gate.

---

## 5. Relationship to Other Documents

| Document | Relationship |
|----------|--------------|
| `docs/10-delivery/23-project-assessment-and-execution-tracker.md` | Source of C-1..C-10 contradiction register, Phase 0–4 execution plan, and evidence inventory; this doc is the **consolidated remaining-items view** |
| `docs/10-delivery/24-strategic-roadmap-and-checklist.md` | Source of P0–P4 strategic recommendations, language policy, and anti-recommendations; this doc encodes the deferred follow-up items |
| `docs/10-delivery/24h-documentation-language-survey-evidence.md` | Source of Tier 3 file inventory (§5.2), ADR/governance README list (§5.3), mixed delivery doc list (§5.4), header-only list (§5.4), and minor fix (§5.6) |
| `docs/10-delivery/22-phase-4-entry-plan.md` | A-01..A-13 detailed tracker; A-09 cross-update is a remaining task (§2.E above) |
| `docs/10-delivery/17-production-readiness-backlog.md` | P0/P1/P2 backlog; C-1 re-verification target |
| `docs/09-operations/10-external-review-packet.md` | C-9 re-verification target |
| `docs/10-delivery/20-project-completion-roadmap.md` | C-8 re-verification target |

---

## 6. Update Log

| Date | Updated By | Changes |
|------|------------|---------|
| 2026-06-10 | BrianNguyen (via authorized assistant fixer) | Initial creation — canonical remaining todo-list from verified audit. Separates completed work (§1), local-executable remaining tasks (§2 A–G), risky/design-first work (§3), and external-gated blockers (§4). No production-readiness claim. No public docs touched. No code changes. External gates (A-03..A-13) remain blocked. A-11 remains deferred/SDK-blocked. |
| 2026-06-10 | BrianNguyen (via authorized assistant fixer) | Low-risk remaining tasks D2/G/F/E executed. D2: `graph.rs` doc-comment "TODO structure" → "placeholder structure". G: explicit `[[bench]]` stanzas added for `query_latency` and `diff_latency` in `intent-service/Cargo.toml` and `rebase-engine/Cargo.toml`. F: CLI short-flag audit revealed `-i` collision in `Run` subcommand (`intent_id` vs `initiated_by`); fixed via `short = 'b'` for `initiated_by`; all help commands exit 0. E: A-09 continuation note added to `22-phase-4-entry-plan.md`; C-8 benchmark wording polished in `20-project-completion-roadmap.md`. D1 webhook TODO deferred/design-first per repo constraints. New evidence doc `docs/10-delivery/25d-low-risk-remaining-tasks-evidence.md` created. No public docs touched. No production claims. External gates remain blocked. |
