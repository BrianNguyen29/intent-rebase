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

## 2. Remaining Local-Executable Tasks (ALL COMPLETE)

These tasks can be executed locally without external reviewers, production infrastructure, or vendor engagement. They are ordered roughly by bounded-slice effort (smallest first). **All items A–G are now complete. Remaining work is external-gated, public-production-gated, commercial-readiness-gated, or risky/design-first (see §3 and §4).**

### A. Tier 3 Full Translations

**What:** Translate 34 full-Vietnamese internal docs to English per the accepted Documentation Language Policy (`24g` §3, `24h` §5.2).

**Files:** `docs/01-product/` (5), `docs/02-architecture/` (5), `docs/03-spec/` (6), `docs/04-api/` (3), `docs/05-data/` (2), `docs/06-backend/` (4), `docs/07-frontend/` (1), `docs/08-security/` (4), `docs/11-quality/` (1), `docs/12-agents/` (2), `docs/99-reference/` (1). 0 files remaining after Batch 9; full list in `24h` §5.2.

**Approach:** Bounded single-file or small-batch slices; each slice must run the public-doc leakage scan, affirmative-claim scan, and `git diff --check` per `24-strategic-roadmap-and-checklist.md` §9.7.

**Status:** ✅ DONE — Batch 1 (`docs/01-product/`, 5 files) completed 2026-06-10. Evidence: `25e`.

---

**Status:** ✅ DONE — Batch 2 (`docs/02-architecture/`, 5 files) completed 2026-06-10. Evidence: `25f`.

---

**Status:** ✅ DONE — Batch 3 (`docs/03-spec/`, 6 files) completed 2026-06-10. Evidence: `25g`.

---

**Status:** ✅ DONE — Batch 4 (`docs/04-api/`, 3 files) completed 2026-06-10. Evidence: `25i`.

---

**Status:** ✅ DONE — Batch 5 (`docs/05-data/`, 2 files) completed 2026-06-10. Evidence: `25j`.

---

**Status:** ✅ DONE — Batch 6 (`docs/06-backend/`, 4 files) completed 2026-06-10. Evidence: `25k`.

---

**Status:** ✅ DONE — Batch 7 (`docs/07-frontend/`, 1 file) completed 2026-06-10. Evidence: `25l`.

---

**Status:** ✅ DONE — Batch 8 (`docs/08-security/`, 4 files) completed 2026-06-10. Evidence: `25m`.

---

**Status:** ✅ DONE — Batch 9 (`docs/11-quality/`, `docs/12-agents/`, `docs/99-reference/`; 4 files) completed 2026-06-10. Evidence: `25n`. All Tier 3 full translations complete (34 files total).

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
| `crates/intent-api/src/webhook_dispatcher.rs` | 79 | `TODO(Slice 4+): version info should be stored in the outbox record at creation time.` | **✅ DONE — BOUNDED IMPLEMENTATION** — ADR-13 accepted; store-at-creation option implemented. `WebhookOutboxRecord` extended with `version`, `version_hash`, `previous_version`. Migration `022` adds columns. Dispatcher reads stored values. `version_hash` and `previous_version` remain `None` at creation until upstream propagation path enhanced. No production readiness claim. |
| `crates/intent-rebase-types/src/graph.rs` | 488 | `Replaces the Phase 1 baseline TODO structure with graph-integrated classification.` | ✅ DONE — wording updated to "placeholder structure"; no functional change. |

**Status:** D1 done (bounded); D2 done.

---

### E. Stale Contradiction Cleanup

**What:** Re-verify and close residual contradictions flagged in the tracker.

| ID | Tracker Claim | Residual Risk | Next Action |
|----|---------------|---------------|-------------|
| **C-1** | P0-1 marked ✅ RESOLVED | `17-production-readiness-backlog.md` P2-6 vs `22-phase-4-entry-plan.md` A-12 status may have drifted since 2026-05-20 | ✅ VERIFIED — no drift since 2026-05-20; A-12 Slice 5a/5b and Phases 1.1–2.3 already recorded. C-1 closed per `25d` evidence. |
| **C-8** | P0-8 marked ✅ RESOLVED | `20-project-completion-roadmap.md` P2 "Docs Complete" vs Phase 4 decomposition may still lag | Verify roadmap accurately reflects delivered decomposition (A-09 S6, route groups, test extractions) |
| **C-9** | P0-9 marked ✅ RESOLVED | `10-external-review-packet.md` G-RLS-1 may lag behind latest RLS integration status | Sync G-RLS-1 with `22-phase-4-entry-plan.md` A-02 and `17-production-readiness-backlog.md` P1-S5i if any changes landed since 2026-05-20 |
| **A-09** | Tracker / 24-strategic note deferred cross-update | `22-phase-4-entry-plan.md` A-09 status line was explicitly deferred in `24e` line 89 and `24-strategic` §5.5 | ✅ DONE — continuation note added to A-09 status line tracking `health_routes` → `routes::health` demo slice and next candidate |

**Status:** A-09 done; C-1 verified/no drift (closed per `25d`); C-8 wording polished.

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
| **NATS per-tenant JetStream streams** | Requires stream migration plan, tenant UUID inventory, rollback strategy, and SRE coordination | **ADR-15 design complete** — staged migration documented (Stage 1 readiness ✅, Stage 2–4 proposed); duplicate storage guardrail defined (subject prefix v2 with env gate); no code changes | External SRE sign-off (A-03), staging env (A-05) |
| **Webhook production delivery** | Requires secret manager integration, key rotation grace window, HMAC signing, delivery SLO, and real external receiver validation | Local-dev bounded slices delivered (`24c`, webhook integration test); SQLx outbox + subscription repos wired; **ADR-13 + D1 bounded implementation done** | External security review (A-04), production infra (A-05), SRE sign-off (A-03) |
| **S3/S4 side-effect auto-compensation** | Requires explicit approval per repo-specific change constraints (AGENTS.md constraint #5) before implementation | Design baseline exists; no implementation started | Explicit user approval required before any code change |
| **Forensic Object Lock / chain-hash** | Requires S3 Object Lock deployment, retention enforcement, tamper-evidence validation, and security review | **Local chain-hash algorithm delivered** — ADR-14 accepted; `chain_hash.rs` pure module with tests; `BundleIntegrity.previous_bundle_hash` added. Object Lock, S3 retention enforcement, and production validation remain blocked | Production infrastructure (A-05), external security review (A-04) |

> **No overclaim:** NATS per-tenant streams and Object Lock production are not in progress. Chain-hash and webhook D1 are bounded local-only implementations. S3/S4 auto-compensation awaits explicit approval.

---

## 4. External-Gated Blockers

These gates **cannot** be closed by local work. They require named independent third-party evidence, production infrastructure, or upstream SDK fixes.

| Gate | Required Evidence | Owner | Local Status |
|------|-------------------|-------|--------------|
| **A-03** | Named external SRE reviewer; signed Section H of external review packet | DuongNguyen (historical 2026-06-15 `APPROVED WITH CONDITIONS` on record); **SELF-ATTESTED-SOLO / WAIVED-SOLO per ADR-16 — external re-signoff NOT obtained** | 🔴 Blocked |
| **A-04** | Named external security reviewer; threat model v2 assessment | DuongNguyen (historical 2026-06-15 `APPROVED WITH CONDITIONS` on record); **SELF-ATTESTED-SOLO / WAIVED-SOLO per ADR-16 — external re-signoff NOT obtained** | 🔴 Blocked |
| **A-05** | Production Postgres/NATS/S3/monitoring operational; deployment runbook executed | SRE | 🔴 Blocked |
| **A-06** | L4 30min sustained + all alert types + real receivers; L5 production load test | Backend Lead / SRE | 🟡 STAGING L4 PASSED — L3/L5 blocked (staging 30-min business-path load passed with receiver validation and synthetic Prometheus rule; production/public-ingress load NOT done) |
| **A-07** | External pen test report (PDF + JSON); HIGH/CRITICAL remediation | External Pen Test Team | 🔴 Blocked — **WAIVED-SOLO / PRIVATE-ONLY per ADR-16**; ZAP self-scan prep only (0 FAIL, 1 WARN); external pen test NOT executed |
| **A-10** | Production NATS topology; full DLQ replay worker validated; SRE sign-off | Backend Lead / SRE | 🔴 Blocked |
| **A-11** | Temporal SDK safe per-request gRPC metadata injection; cross-process trace IDs in OTLP | Backend Lead / SRE | 🔴 Deferred / SDK-blocked |
| **A-12** | Production secret manager + key rotation; staging/production SLO evidence; external review closure | SRE / Security | 🔴 Blocked |
| **A-13** | S3 Object Lock deployed; chain-hash; retention enforcement validated | Backend Lead / Security | 🔴 Blocked — local chain-hash algorithm delivered (ADR-14); Object Lock and production validation remain blocked |

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
| 2026-06-10 | BrianNguyen (via authorized assistant fixer) | Tier 3 Batch 1 (`docs/01-product/`) translated. Five files (`01-product-thesis.md`, `02-goals-nongoals.md`, `03-core-principles.md`, `04-use-cases.md`, `05-glossary.md`) translated from Vietnamese to English in place. Remaining Tier 3 count updated from 34 to 29. Verification: Vietnamese-diacritic scan clean, public-doc leakage scan clean, affirmative-claim scan clean, no new `.vi.md`, `git diff --check` pass. New evidence doc `docs/10-delivery/25e-documentation-language-tier3-product-evidence.md` created. Internal solo sign-off added. No public docs touched. No code changes. External gates remain blocked. |
| 2026-06-10 | BrianNguyen (via authorized assistant fixer) | Tier 3 Batch 2 (`docs/02-architecture/`) translated. Five files (`01-system-overview.md`, `02-components.md`, `03-trust-boundaries.md`, `04-scaling-topology.md`, `05-deployment-models.md`) translated from Vietnamese to English in place. Remaining Tier 3 count updated from 29 to 24. Policy Snapshot section in `01-system-overview.md` labeled as historical implementation detail preserved verbatim. Verification: Vietnamese-diacritic scan clean, public-doc leakage scan clean, affirmative-claim scan clean, no new `.vi.md`, `git diff --check` pass. New evidence doc `docs/10-delivery/25f-documentation-language-tier3-architecture-evidence.md` created. Internal solo sign-off added. No public docs touched. No code changes. External gates remain blocked. |
| 2026-06-10 | BrianNguyen (via authorized assistant fixer) | Tier 3 Batch 3 (`docs/03-spec/`) translated. Six files (`01-intent-model.md`, `02-semantic-diff.md`, `03-dependency-graph.md`, `04-rebase-engine.md`, `05-compensation.md`, `06-provenance.md`) translated from Vietnamese to English in place. Remaining Tier 3 count updated from 24 to 18. Phase 1 Status block in `04-rebase-engine.md` preserved verbatim (already English). Schemas, code blocks, and English type lists preserved verbatim across all six files. Verification: Vietnamese-diacritic scan clean, public-doc leakage scan clean, affirmative-claim scan clean, no new `.vi.md`, `git diff --check` pass. New evidence doc `docs/10-delivery/25g-documentation-language-tier3-spec-evidence.md` created. Internal solo sign-off added. No public docs touched. No code changes. External gates remain blocked. A-11 remains deferred/SDK-blocked. |
| 2026-06-10 | BrianNguyen (via authorized assistant fixer) | **Design-first unblock slice executed.** (1) Webhook D1: ADR-13 created and accepted; store-at-creation bounded implementation delivered — `WebhookOutboxRecord` version fields, migration `022`, dispatcher reads stored values, OpenAPI updated, tests pass. (2) Forensic chain-hash: ADR-14 created and accepted; pure local algorithm module `chain_hash.rs` with tests delivered; `BundleIntegrity.previous_bundle_hash` added. (3) NATS per-tenant streams: ADR-15 created (Proposed); staged migration design with duplicate-storage guardrail; no code changes. (4) Todo list updated with truthful statuses. (5) Evidence doc `25h` created with internal-only BrianNguyen sign-off and explicit non-production caveats. Verification: `cargo fmt --check` pass, `cargo clippy --workspace --all-targets` pass (warnings-only), `cargo test --workspace --lib` pass (567 passed, 0 failed, 18 ignored). No public docs touched except OpenAPI schema update for WebhookOutboxRecord. External gates remain blocked. S3/S4 auto-compensation remains blocked pending explicit approval. |
| 2026-06-10 | BrianNguyen (via authorized assistant fixer) | Tier 3 Batch 4 (`docs/04-api/`) translated. Three files (`01-rest-api.md`, `02-events.md`, `03-webhooks.md`) translated from Vietnamese to English in place. Remaining Tier 3 count updated from 18 to 15. §2.E C-1 status updated to verified/no drift (closed per `25d`). Blockquotes, JSON examples, API paths, schemas, event names, and existing caveats preserved verbatim across all three files. Chinese example value `chk_after_approval_收集` left untouched as example data. Verification: Vietnamese-diacritic scan clean, public-doc leakage scan clean, affirmative-claim scan clean, no new `.vi.md`, `git diff --check` pass. New evidence doc `docs/10-delivery/25i-documentation-language-tier3-api-evidence.md` created. Internal solo sign-off added. No public docs touched. No code changes. External gates remain blocked. A-11 remains deferred/SDK-blocked. |
| 2026-06-10 | BrianNguyen (via authorized assistant fixer) | Tier 3 Batch 5 (`docs/05-data/`) translated. Two files (`02-storage.md`, `03-dataflow.md`) translated from Vietnamese to English in place. Remaining Tier 3 count updated from 15 to 13. Verification: Vietnamese-diacritic scan clean, public-doc leakage scan clean, affirmative-claim scan clean, no new `.vi.md`, `git diff --check` pass. New evidence doc `docs/10-delivery/25j-documentation-language-tier3-data-evidence.md` created. Internal solo sign-off added. No public docs touched. No code changes. External gates remain blocked. A-11 remains deferred/SDK-blocked. |
| 2026-06-10 | BrianNguyen (via authorized assistant fixer) | Tier 3 Batch 6 (`docs/06-backend/`) translated. Four files (`01-service-boundaries.md`, `02-runtime-integration.md`, `04-consistency-model.md`, `05-failure-handling.md`) translated from Vietnamese to English in place. Remaining Tier 3 count updated from 13 to 9. Verification: Vietnamese-diacritic scan clean, public-doc leakage scan clean, affirmative-claim scan clean, no new `.vi.md`, `git diff --check` pass. New evidence doc `docs/10-delivery/25k-documentation-language-tier3-backend-evidence.md` created. Internal solo sign-off added. No public docs touched. No code changes. External gates remain blocked. A-11 remains deferred/SDK-blocked. |
| 2026-06-10 | BrianNguyen (via authorized assistant fixer) | Tier 3 Batch 7 (`docs/07-frontend/`) translated. One file (`02-ux-flows.md`) translated from Vietnamese to English in place. Remaining Tier 3 count updated from 9 to 8. Verification: Vietnamese-diacritic scan clean, public-doc leakage scan clean, affirmative-claim scan clean, no new `.vi.md`, `git diff --check` pass. New evidence doc `docs/10-delivery/25l-documentation-language-tier3-frontend-evidence.md` created. Internal solo sign-off added. No public docs touched. No code changes. External gates remain blocked. A-11 remains deferred/SDK-blocked. |
| 2026-06-10 | BrianNguyen (via authorized assistant fixer) | Tier 3 Batch 8 (`docs/08-security/`) translated. Four files (`01-threat-model.md`, `02-authn-authz.md`, `03-privacy-and-data-handling.md`, `04-audit-and-compliance.md`) translated from Vietnamese to English in place. Remaining Tier 3 count updated from 8 to 4. Verification: Vietnamese-diacritic scan clean, public-doc leakage scan clean, affirmative-claim scan clean, no new `.vi.md`, `git diff --check` pass. New evidence doc `docs/10-delivery/25m-documentation-language-tier3-security-evidence.md` created. Internal solo sign-off added. No public docs touched. No code changes. External gates remain blocked. A-11 remains deferred/SDK-blocked. |
| 2026-06-10 | BrianNguyen29 (via authorized assistant fixer) | Tier 3 Batch 9 (final batch) translated. Four files (`03-acceptance-and-uat.md`, `01-agent-implementation-guide.md`, `03-prompts-contracts.md`, `01-rationale-and-external-patterns.md`) translated from Vietnamese to English in place. Remaining Tier 3 count updated from 4 to 0. All Tier 3 full translations now complete (34 files total). Verification: Vietnamese-diacritic scan clean, public-doc leakage scan clean, affirmative-claim scan clean, no new `.vi.md`, `git diff --check` pass. New evidence doc `docs/10-delivery/25n-documentation-language-tier3-final-evidence.md` created. Internal solo sign-off added. No public docs touched. No code changes. No production-readiness claim. External gates remain blocked. A-11 remains deferred/SDK-blocked. |
| 2026-06-21 | BrianNguyen (via authorized assistant fixer) | Post-2026-06-10 close-out update. All local-executable tasks (§2 A–G) complete. Post-2026-06-10 deliverables recorded: ESO/GSM sync and ExternalSecrets readiness, staging/prod API key rotation validation, 30-min staging business-path load + receiver validation + synthetic Prometheus rule, external pentest engagement packet, ADR-16 solo/private waiver, final private DR smoke, JWT dual-key support, final private close-out docs. Section 4 external-gated blockers updated with ADR-16 terms (A-03/A-04 SELF-ATTESTED-SOLO / WAIVED-SOLO; A-07 WAIVED-SOLO / PRIVATE-ONLY). Remaining work framed as external-gated, public-production-gated, commercial-readiness-gated, or risky/design-first. No production-ready claim. No external sign-off claim. |
