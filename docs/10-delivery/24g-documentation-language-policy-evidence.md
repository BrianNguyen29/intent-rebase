# P2 Documentation Language Policy — Evidence

> **Status:** POLICY ACCEPTED — 2026-06-07
> **Owner:** BrianNguyen29 (Backend Lead, solo practitioner)
> **Scope:** Internal evidence for the P2 Documentation Language Policy decision. Records the user-accepted policy, the rationale, the migration checklist, and the future-bounded-slice guardrails. **This slice records the policy and the next-step checklist only; it does not perform any actual doc migration.** Actual migrations are future bounded slices.
> **Non-Production Caveat:** This document is an internal planning artifact. It is not a public support document, it does not constitute production-readiness evidence, it does not claim CI-green status, and it does not claim external sign-off. No code change is associated with this slice. All external and production evidence gates remain blocked or deferred.

---

## 1. Purpose

The strategic roadmap (`docs/10-delivery/24-strategic-roadmap-and-checklist.md` §9) called for a P2 Documentation Language Policy decision because the repo mixes English and Vietnamese in internal docs (e.g., `docs/10-delivery/01-roadmap.md` has English headers with Vietnamese phase bullets, `docs/13-adrs/01-runtime-adapter.md` lines 25 and 38 contain Vietnamese content in an English-doc'd ADR, `docs/10-delivery/04-phase-2-runtime-integrated.md` line 4 has a Vietnamese phase description). The strategic evaluation flagged this as a future-contributor confusion risk. The §9.5 wording was framed as a **recommended** policy, pending a user decision.

This evidence doc closes that decision: the user (BrianNguyen29) **accepted** option A from the policy options as the canonical Documentation Language Policy, recorded in `24-strategic-roadmap-and-checklist.md` §9.5 (now marked "Accepted Policy (user decision on 2026-06-07)").

The acceptance records:
- the policy text (verbatim);
- the migration checklist for the *future* bounded slices that will execute the migration;
- the acceptance criteria a future slice owner must meet before claiming the migration done;
- the guardrails that prevent leakage or overclaim during migration.

No files were migrated, renamed, translated, or deleted by this slice. No public docs were touched.

---

## 2. User Decision (Recorded 2026-06-07)

| Field | Value |
|-------|-------|
| Decision | **A. English-primary technical docs** — Public/technical docs use English; Vietnamese is allowed only in `README.vi.md` or explicit `*.vi.md` translation files. |
| Decided by | BrianNguyen29 (user) |
| Decided on | 2026-06-07 |
| Recorded in | `docs/10-delivery/24-strategic-roadmap-and-checklist.md` §9.5 (rewritten from "Recommended Policy (proposed, pending user decision)" to "Accepted Policy (user decision on 2026-06-07)"). |
| Tracker log | `docs/10-delivery/23-project-assessment-and-execution-tracker.md` §13 (2026-06-07 row). |
| Companion update-log row | `docs/10-delivery/24-strategic-roadmap-and-checklist.md` §14 (2026-06-07 row). |

The full policy text is reproduced verbatim in §3 below.

---

## 3. Accepted Policy (Verbatim)

> The following is the canonical policy text, reproduced verbatim from `24-strategic-roadmap-and-checklist.md` §9.5.

1. **Primary technical documentation language: English.** All public docs, internal ADRs (`docs/13-adrs/`), internal delivery docs (`docs/10-delivery/`), internal ops docs (`docs/09-operations/`), and any new technical doc surface written for maintainers, reviewers, or future contributors are in English by default.
2. **Vietnamese is allowed only in the following surfaces:**
   - `README.vi.md` (the existing bilingual top-level Vietnamese readme).
   - Explicit translation files with the `.vi.md` suffix (e.g., `docs/getting-started/quickstart.vi.md` as a paired translation of `docs/getting-started/quickstart.md`).
   - Quoted user input or historical/internal evidence where translation would change meaning. Such quotes **must be labeled** (e.g., `> Original user input (Vietnamese, preserved verbatim):`) so a reader knows the content is preserved-as-quoted, not authored-in-Vietnamese.
3. **Public docs and technical docs should avoid mixed-language sections in the same file.** A single file should be either English or Vietnamese — not both. The only file-level exception is `README.vi.md` (which is, by name and intent, a Vietnamese counterpart of `README.md`).
4. **Future migrations must be bounded, reviewed, and verify no public/internal leakage or overclaim.** A migration slice is acceptable only if:
   - It is a single bounded slice with a written action checklist in `24-strategic-roadmap-and-checklist.md` (or an equivalent internal planning doc) **before** any doc is touched.
   - It runs the public-doc leakage scan and the affirmative-claim scan in §5 of this evidence doc before claiming done.
   - It records the diff (file paths + number of lines translated / labeled / quoted) in an evidence doc (`docs/10-delivery/24h-…-evidence.md` or similar, following the `24b`–`24g` naming pattern).
   - It does **not** introduce a new top-level `.vi.md` file outside the explicit allowlist in item 2.
   - It does **not** translate public docs (`README.md`, `README.vi.md`, `docs/README.md`, `docs/getting-started/`, `docs/reference/`, `.github/`, `CONTRIBUTING`, `SECURITY`) except via the explicit `.vi.md` translation-file pattern.
   - It does **not** delete any existing Vietnamese content without a bounded "label as quoted historical evidence" replacement, in line with item 2's quoted-evidence allowance.

> **Why a policy rather than a one-off rewrite:** a one-off rewrite erases historical context and creates a hidden tension between "preserve decision rationale" and "standardize language". The policy above lets future bounded slices decide case-by-case whether a doc should be translated in full, translated in part, or labeled as quoted Vietnamese evidence — while keeping the public reading order in English and not blocking non-Vietnamese-speaking contributors.

---

## 4. Migration Checklist (For Future Bounded Slices)

> **Status:** This checklist is **planned, not executed**. Each item below is a future bounded slice. The current slice records the policy; the migration is deferred.

The future bounded slices must, in order:

1. **Survey.** Run a workspace-wide scan of every `.md` file and report which files contain Vietnamese content. The canonical scan is `grep -rPl "[\\xC0-\\xFF]" --include="*.md" docs README.md README.vi.md`. The expected output is a list of files that need triage (the exact file list is left for the survey slice to capture; it is out of scope for the policy-acceptance slice).
2. **Triage.** For each file containing Vietnamese content, classify into one of:
   - **Translate in full** to English (for ADRs, delivery docs, ops docs).
   - **Pair with `.vi.md`** (only if a future maintainer explicitly wants a Vietnamese translation; the default is **not** to add a translation file).
   - **Label as quoted historical evidence** (for content where translation would change meaning — typically user-decision rationales, command outputs preserved verbatim, or sign-off text).
3. **Prioritize.** Start with the lowest-risk / highest-noise items, not the largest files. The natural first bounded slice is the 3-file "wording correction" mentioned in `24-strategic-roadmap-and-checklist.md` §9.1: `docs/13-adrs/01-runtime-adapter.md` (lines 25, 38), `docs/10-delivery/04-phase-2-runtime-integrated.md` (line 4), and `docs/10-delivery/01-roadmap.md` (the mixed English/Vietnamese phase bullets).
4. **Translate in a single bounded slice per file** (or per group of related files if a slice owner judges the risk low). Do not bundle a "translate everything" slice; that would violate the "bounded, reviewed" guardrail in item 4 of §3.
5. **Verify** with the public-doc leakage scan (§5.1), the affirmative-claim scan (§5.2), and the "no new `.vi.md`" check (§5.3) before claiming the slice done.
6. **Record** the diff in an evidence doc following the `24b`–`24g` naming pattern. The natural filename for the first migration slice is `24h-docs-migration-step-1-evidence.md` (or a similar bounded name). The evidence doc must record:
   - The list of files changed (path + line ranges or stable module anchors).
   - The classification per file (translate / pair / label).
   - The verification commands and their results.
   - The non-production caveat paragraph (this evidence doc's §6 boilerplate is a good template).

> **Do not migrate these files in this scope:**
> - `README.md`, `README.vi.md` — public-facing; bilingual by design; out of policy scope (item 1 says English is the primary technical doc language; `README.vi.md` is the explicit bilingual counterpart named in item 2's allowlist).
> - `docs/README.md`, `docs/getting-started/`, `docs/reference/`, `.github/`, `CONTRIBUTING`, `SECURITY` — public-facing; already in English per the `b9289e2` public-doc refresh. No changes required.
> - Historical artifacts in `docs/10-delivery/checklists/checklist-phase-*.md` — these are historical phase checklists preserved as-is per the existing roadmap precedent.

---

## 5. Verification (For Future Bounded Slices)

> These commands are **recorded** for future bounded slices; they are **not** run by this policy-acceptance slice (it is a docs-only acceptance row with no migration in scope).

### 5.1 Public-Doc Leakage Scan

```bash
# Forbidden Vietnamese strings in public docs (affirmative only; see §5.2 for the
# claim-language scan). Translations of public docs (e.g. README.vi.md) are excluded.
# Phase / week words are the highest-signal markers in the current repo.
grep -rP "tuần|tiếng việt|đã|được|chưa|ADR-0[0-9]" \
  README.md docs/README.md \
  docs/getting-started/*.md docs/reference/*.md 2>/dev/null

# Expected: no matches. If a match appears, the file is leaking Vietnamese content
# into a public surface and the migration slice must fix it before claiming done.
# Negations and explicit bilingual labels (e.g. "(Vietnamese, preserved verbatim)")
# are acceptable and should not be flagged.
```

### 5.2 Affirmative-Claim Scan (forbidden in public docs)

```bash
# Forbidden positive-affirmation phrases in public docs
grep -nPi "is production[- ]ready|claim.*production[- ]ready|production[- ]ready\s+(system|service|build|cut|claim|signal)|CI[- ]green|external sign[- ]off obtained|pen test passed" \
  README.md README.vi.md docs/README.md \
  docs/getting-started/*.md docs/reference/*.md

# Expected: no matches. "X is NOT production-ready" / "không phải ... production-ready"
# are explicit safety disclaimers and are correct.
```

### 5.3 No New `.vi.md` Outside Allowlist

```bash
# Files matching the .vi.md pattern. The policy's allowlist is README.vi.md and
# any explicit translation file with the .vi.md suffix. This command lists every
# such file so a migration slice can confirm it is not creating new ones by
# accident.
find . -name "*.vi.md" -not -path "./target/*" -not -path "./node_modules/*"

# Expected (as of 2026-06-07): ./README.vi.md. If a future bounded slice adds
# more .vi.md files, it must (a) be intentional, (b) be a paired translation of
# an English .md file with the same basename, and (c) be recorded in the
# evidence doc with the paired English file path.
```

### 5.4 Diff Hygiene

```bash
git diff --check
```

---

## 6. Non-Production Caveat (Boilerplate For Future Migration Evidence Docs)

> The following is a template that future bounded migration evidence docs may copy verbatim. The wording is intentionally identical to the boilerplate used by `24b`–`24f` so the safety disclaimer is consistent across the repo.

- This slice is a **doc-translation/migration** slice, not a public-doc rewrite. The migration is **internal-only** unless explicitly called out in the slice handoff as a public-doc refresh.
- This slice is **not** a production-readiness claim. External gates (A-03, A-04, A-05, A-06, A-07, A-10, A-12, A-13) remain blocked. A-11 remains deferred/SDK-blocked.
- This slice is **not** a CI-green claim. Per `17-production-readiness-backlog.md` P0-1 and `22-phase-4-entry-plan.md` A-01, GitHub Actions CI is intentionally disabled by design.
- No code change is associated with this slice. `cargo fmt --all -- --check`, `cargo check --workspace --all-features`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace --lib --all-features` are **not** run by a doc-migration slice; they are left for the next code-touching bounded slice.
- No public doc was modified unless the slice handoff explicitly authorizes it. The default scope of a doc-migration slice is **internal-only** (`docs/10-delivery/`, `docs/13-adrs/`, `docs/09-operations/`, and similar).

---

## 7. Relationship to Other Documents

| Document | Relationship |
|----------|--------------|
| `docs/10-delivery/24-strategic-roadmap-and-checklist.md` §9 | P2 Documentation Language Policy section; §9.5 wording rewritten from "Recommended Policy (proposed, pending user decision)" to "Accepted Policy (user decision on 2026-06-07)"; §3 status row flipped from `⬜ Not started` to `✅ ACCEPTED (user decision)`. |
| `docs/10-delivery/23-project-assessment-and-execution-tracker.md` §13 | Update log gains a new row recording this policy acceptance. |
| `docs/10-delivery/24b-…-evidence.md` … `24f-…-evidence.md` | Sibling evidence docs following the same `24x-…-evidence.md` naming pattern; this doc is the first **policy** evidence doc in the series (no code change, no verification command). |
| `docs/10-delivery/01-roadmap.md` | One of the three files called out in `24-strategic-roadmap-and-checklist.md` §9.1 as a known mixed-language offender; the future migration slice will triage this file per §4. |
| `docs/13-adrs/01-runtime-adapter.md` | One of the three files called out in §9.1; future migration slice will triage lines 25, 38. |
| `docs/10-delivery/04-phase-2-runtime-integrated.md` | One of the three files called out in §9.1; future migration slice will triage line 4. |
| `README.md`, `README.vi.md` | Public top-level readmes; `README.vi.md` is the explicit bilingual counterpart named in §3 item 2's allowlist. **Not modified** by this slice. |
| `docs/README.md`, `docs/getting-started/`, `docs/reference/` | Public support docs; already in English per the `b9289e2` public-doc refresh. **Not modified** by this slice. |
| `AGENTS.md` | Author/agent rules; not modified by this slice. The policy is internal-only and is referenced from `24-strategic-roadmap-and-checklist.md` §9 + this evidence doc, not from `AGENTS.md`. |

---

## 8. Update Log

| Date | Updated By | Changes |
|------|------------|---------|
| 2026-06-07 | BrianNguyen29 (via authorized assistant fixer) | Initial creation — records the P2 Documentation Language Policy user decision (option A: English-primary technical docs; Vietnamese allowed only in `README.vi.md` or explicit `*.vi.md` translation files). §3 reproduces the policy verbatim. §4 is a future-slice migration checklist (translate / pair / label classification; bounded per-file slices; lowest-risk first). §5 records the public-doc leakage scan, the affirmative-claim scan, the no-new-`.vi.md` check, and `git diff --check`. §6 is a non-production boilerplate for future migration evidence docs. No doc files were translated, renamed, or deleted by this slice; no public docs were touched; no code changes. The actual migration is **deferred to future bounded slices** per the policy's "Future migrations must be bounded, reviewed, and verify no public/internal leakage or overclaim" guardrail. No production-readiness claim. External gates (A-03..A-13) remain blocked. A-11 remains deferred/SDK-blocked. |
---

## Sign-Off

**Signed:** BrianNguyen29 (via authorized assistant), internal documentation slice only.

> This sign-off is internal planning/evidence documentation only. It does **not** constitute external review, production sign-off, or CI-green attestation. No external or production gates are claimed closed by this signature.
