# P2 Documentation Language Policy — Survey Evidence

> **Status:** SURVEY COMPLETE — 2026-06-08
> **Owner:** BrianNguyen (Backend Lead, solo practitioner)
> **Scope:** Internal evidence for the P2 Documentation Language Policy workspace-wide survey. Records the scan method, file counts, public-doc verdict, classification inventory, priority tiers, and next-slice recommendations. **This slice is survey-only; it does not perform any actual doc migration.** Actual migrations are future bounded slices.
> **Non-Production Caveat:** This document is an internal planning artifact. It is not a public support document, it does not constitute production-readiness evidence, it does not claim CI-green status, and it does not claim external sign-off. No code change is associated with this slice. All external and production evidence gates remain blocked or deferred.

---

## 1. Purpose

The strategic roadmap (`docs/10-delivery/24-strategic-roadmap-and-checklist.md` §9.6.b) listed a **survey slice** as the first future bounded slice under the Documentation Language Policy. This evidence doc closes that slice: it records the full workspace scan, the classification of every file that contains Vietnamese content, and the recommended priority order for future migration slices.

No files were migrated, renamed, translated, or deleted by this slice. No public docs were touched.

---

## 2. Survey Method

| Field | Value |
|-------|-------|
| Command | `rg -c "[à-ỹÀ-ỸĐđ]" --include="*.md" -g '!target/' -g '!.git/' -g '!node_modules/'` |
| Tool | `ripgrep` (`rg`) — recursive, parallel, respects `.gitignore` |
| Pattern | Vietnamese diacritics character class `[à-ỹÀ-ỸĐđ]` |
| Scope | All `*.md` files in the workspace |
| Exclusions | `target/`, `.git/`, `node_modules/` |
| Date | 2026-06-08 |

---

## 3. Counts

| Metric | Value |
|--------|-------|
| Total `*.md` files scanned | 159 |
| Files with Vietnamese diacritics | 70 |
| Files clean (no Vietnamese diacritics) | 89 |

---

## 4. Public-Doc Verdict

**PASS.**

| Doc Surface | Hits | Verdict |
|-------------|------|---------|
| `README.md` | 1 | **Allowed** — intentional language-toggle link `[Tiếng Việt](README.vi.md)` |
| `README.vi.md` | 86 | **Allowed** — explicit `.vi.md` bilingual counterpart per policy §3 item 2 |
| `docs/README.md` | 0 | Clean |
| `docs/getting-started/*.md` | 0 | Clean |
| `docs/reference/*.md` | 0 | Clean |
| `.github/**/*.md` | 0 | Clean |
| `CONTRIBUTING.md` | 0 | Clean |
| `SECURITY.md` | 0 | Clean |

---

## 5. Classification Inventory

> **Note:** Category totals are indicative; a small number of files may appear in more than one list (e.g., ADR/governance docs that are also referenced from other sections). The lists below are presented as surveyed, without forced de-duplication, to preserve the exact scan output.

### 5.1 Allowed Policy Evidence Docs (labeled/quoted context — no migration needed)

These files contain Vietnamese text only as labeled policy context, scan commands, or quoted evidence. They are **not migration targets**.

| File | Approx. Hits | Reason |
|------|-------------|--------|
| `docs/10-delivery/24-strategic-roadmap-and-checklist.md` | 6 | Labeled/quoted policy context (§9.5, §9.1 examples) |
| `docs/10-delivery/24g-documentation-language-policy-evidence.md` | 2 | Scan commands and labeled evidence |

### 5.2 Full Vietnamese Internal Docs — Translation Candidates (34 files)

These files contain substantial Vietnamese prose and are the primary targets for future bounded translation slices.

```
docs/01-product/01-product-thesis.md
docs/01-product/02-goals-nongoals.md
docs/01-product/03-core-principles.md
docs/01-product/04-use-cases.md
docs/01-product/05-glossary.md
docs/02-architecture/01-system-overview.md
docs/02-architecture/02-components.md
docs/02-architecture/03-trust-boundaries.md
docs/02-architecture/04-scaling-topology.md
docs/02-architecture/05-deployment-models.md
docs/03-spec/01-intent-model.md
docs/03-spec/02-semantic-diff.md
docs/03-spec/03-dependency-graph.md
docs/03-spec/04-rebase-engine.md
docs/03-spec/05-compensation.md
docs/03-spec/06-provenance.md
docs/04-api/01-rest-api.md
docs/04-api/02-events.md
docs/04-api/03-webhooks.md
docs/05-data/02-storage.md
docs/05-data/03-dataflow.md
docs/06-backend/01-service-boundaries.md
docs/06-backend/02-runtime-integration.md
docs/06-backend/04-consistency-model.md
docs/06-backend/05-failure-handling.md
docs/07-frontend/02-ux-flows.md
docs/08-security/01-threat-model.md
docs/08-security/02-authn-authz.md
docs/08-security/03-privacy-and-data-handling.md
docs/08-security/04-audit-and-compliance.md
docs/11-quality/03-acceptance-and-uat.md
docs/12-agents/01-agent-implementation-guide.md
docs/12-agents/03-prompts-contracts.md
docs/99-reference/01-rationale-and-external-patterns.md
```

### 5.3 ADR / Governance READMEs — Translation Candidates (3 files)

```
docs/13-adrs/README.md
docs/14-governance/README.md
docs/13-adrs/01-runtime-adapter.md
```

### 5.4 Header-Only Minimal Fixes (15 files)

These files are predominantly English but contain a small number of Vietnamese headers or short phrases. A future bounded slice can fix them with minimal line changes.

```
docs/14-governance/02-provenance-spec.md
docs/14-governance/04-approval-revalidation.md
docs/14-governance/05-immutable-retention-tamper-resistance.md
docs/14-governance/06-threat-model-v2.md
docs/14-governance/07-authz-matrix.md
docs/14-governance/08-tenant-isolation.md
docs/14-governance/09-data-handling-redaction.md
docs/14-governance/10-forensic-bundle.md
docs/14-governance/11-incident-freeze.md
docs/14-governance/12-replay-compatibility.md
docs/14-governance/13-residual-risk-spec.md
docs/14-governance/14-incident-response-plan.md
docs/08-security/05-compliance-checklist.md
docs/08-security/06-pen-test-scope.md
docs/08-security/07-data-retention-verification.md
```

### 5.5 Mixed Delivery Docs — Translation / Classification Needed (12 files)

These files mix English and Vietnamese at the section or bullet level. A future slice should classify each section as **translate**, **pair**, or **label** before editing.

```
docs/10-delivery/01-roadmap.md
docs/10-delivery/02-phase-0-foundations.md
docs/10-delivery/03-phase-1-core-control-plane.md
docs/10-delivery/04-phase-2-runtime-integrated.md
docs/10-delivery/06-phase-4-expansion.md
docs/10-delivery/11-phase-2b-sign-off-packet.md
docs/10-delivery/checklists/README.md
docs/10-delivery/checklists/checklist-phase-0.md
docs/10-delivery/checklists/checklist-phase-1.md
docs/10-delivery/checklists/checklist-phase-2.md
docs/10-delivery/checklists/checklist-phase-3.md
docs/10-delivery/checklists/checklist-phase-4.md
```

### 5.6 Minor Single-Word Fix Candidate (1 file)

| File | Line | Current | Future Action |
|------|------|---------|---------------|
| `docs/09-operations/03-observability.md` | 28 | `Structured logs với:` | Replace `với` with English equivalent in a future migration slice |

---

## 6. Priority Tiers

| Tier | Files | Rationale |
|------|-------|-----------|
| **Tier 1** | `docs/13-adrs/01-runtime-adapter.md`, `docs/10-delivery/04-phase-2-runtime-integrated.md`, `docs/10-delivery/01-roadmap.md` | Highest visibility / most frequently referenced internal docs; lowest risk because they are small or already partially English |
| **Tier 2** | Header-only `## Mục đích` fixes (§5.4) | Bounded, low line count, high cosmetic value |
| **Tier 3** | Full translations (§5.2, §5.3, §5.5) | Larger effort; should be split into multiple single-file or small-batch bounded slices |

---

## 7. Next-Slice Recommendations

1. **Bounded slice: Tier 1 triage.** Pick one Tier 1 file (recommendation: `docs/13-adrs/01-runtime-adapter.md`), classify every Vietnamese segment as translate / pair / label, execute the change, and record the diff in an evidence doc (`24i-…` or next in series). *(Completed 2026-06-08: all three Tier 1 files delivered. `docs/13-adrs/01-runtime-adapter.md` classified as `translate` and executed in full — evidence: `24i`. `docs/10-delivery/04-phase-2-runtime-integrated.md` and `docs/10-delivery/01-roadmap.md` translated — evidence: `24j`.)*
2. **Bounded slice: Tier 2 header sweep.** Fix all 15 header-only files in a single slice if the total diff is < 50 lines; otherwise split into two slices. *(Completed 2026-06-08: 15 files, 16 lines changed — evidence: `24j`.)*
3. **Bounded slice: Tier 3 batch 1.** Pick the first 3–5 full-translation files from §5.2 (recommendation: start with `docs/01-product/` in numerical order) and translate them in a single bounded slice. *(Completed 2026-06-10: `docs/01-product/01-product-thesis.md`, `02-goals-nongoals.md`, `03-core-principles.md`, `04-use-cases.md`, `05-glossary.md` translated — evidence: `25e`.)*
4. **Bounded slice: Tier 3 batch 2.** Continue with the next 3–5 full-translation files from §5.2 (recommendation: `docs/02-architecture/` in numerical order). *(Completed 2026-06-10: `docs/02-architecture/01-system-overview.md`, `02-components.md`, `03-trust-boundaries.md`, `04-scaling-topology.md`, `05-deployment-models.md` translated — evidence: `25f`. Remaining Tier 3 count: 24 files.)*
4. **Per-slice guardrails** (mandatory, per policy §9.7):
   - Run the public-doc leakage scan and affirmative-claim scan before claiming done.
   - Record classification (translate / pair / label) per file in the evidence doc.
   - Do not modify `README.md`, `README.vi.md`, `docs/README.md`, `docs/getting-started/`, `docs/reference/`, `.github/`, `CONTRIBUTING`, `SECURITY`.
   - Include a non-production caveat paragraph in the evidence doc.

---

## 8. Non-Production Caveat

- This slice is a **survey** slice, not a migration slice. No file was translated, renamed, or deleted.
- This slice is **not** a production-readiness claim. External gates (A-03, A-04, A-05, A-06, A-07, A-10, A-12, A-13) remain blocked. A-11 remains deferred/SDK-blocked.
- This slice is **not** a CI-green claim. GitHub Actions CI is intentionally disabled by design.
- No code change is associated with this slice. No `cargo` commands were run.
- No public doc was modified. The default scope of this slice is **internal-only**.

---

## 9. Relationship to Other Documents

| Document | Relationship |
|----------|--------------|
| `docs/10-delivery/24-strategic-roadmap-and-checklist.md` §9 | P2 Documentation Language Policy section; §9.6.b survey item marked complete by this slice. |
| `docs/10-delivery/24g-documentation-language-policy-evidence.md` | Parent policy evidence doc; this doc is the first **survey** evidence doc in the series. |
| `docs/10-delivery/23-project-assessment-and-execution-tracker.md` §13 | Update log gains a new row recording this survey completion. |
| `README.md`, `README.vi.md`, `docs/README.md`, `docs/getting-started/`, `docs/reference/`, `.github/`, `CONTRIBUTING`, `SECURITY` | **Not modified** by this slice. Public-doc verdict is PASS. |

---

## 10. Update Log

| Date | Updated By | Changes |
|------|------------|---------|
| 2026-06-08 | BrianNguyen (via authorized assistant fixer) | Initial creation — records the P2 Documentation Language Policy workspace-wide survey: scan method (`rg -c` for Vietnamese diacritics), counts (159 total `*.md`, 70 with hits), public-doc verdict (PASS), full classification inventory (34 full-translation files, 3 ADR/governance READMEs, 15 header-only fixes, 12 mixed delivery docs, 1 minor single-word fix), priority tiers (Tier 1/2/3), and next-slice recommendations. No files were translated, renamed, or deleted; no public docs were touched; no code changes. Non-production caveat preserved. External gates (A-03..A-13) remain blocked. A-11 remains deferred/SDK-blocked. |
| 2026-06-10 | BrianNguyen (via authorized assistant fixer) | Tier 3 Batch 2 (`docs/02-architecture/`) completion note added. Five files (`01-system-overview.md`, `02-components.md`, `03-trust-boundaries.md`, `04-scaling-topology.md`, `05-deployment-models.md`) translated from Vietnamese to English in place. Next-slice recommendation item 4 added. Remaining Tier 3 count: 24 files. No public docs touched. No code changes. External gates remain blocked. |
