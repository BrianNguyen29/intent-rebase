# P2 Documentation Language Policy — Tier 1 + Tier 2 Migration Evidence

> **Status:** MIGRATION SLICE 1 COMPLETE (Tier 1) + TIER 2 HEADER SWEEP COMPLETE — 2026-06-08
> **Owner:** BrianNguyen (Backend Lead, solo practitioner)
> **Scope:** Internal evidence for the Documentation Language Policy Tier 1 file translations and Tier 2 header-only sweep. Records changed files, classification, verification commands, and caveats. **This slice does not claim completion of any Tier 3 or remaining mixed-delivery docs.**
> **Non-Production Caveat:** This document is an internal planning artifact. It is not a public support document, it does not constitute production-readiness evidence, it does not claim CI-green status, and it does not claim external sign-off. No code change is associated with this slice. All external and production evidence gates remain blocked or deferred.

---

## 1. Purpose

This evidence doc closes the second bounded migration slice under the P2 Documentation Language Policy (accepted 2026-06-07, see `24g-documentation-language-policy-evidence.md`). It records:

- Tier 1 file translations (`docs/10-delivery/04-phase-2-runtime-integrated.md`, `docs/10-delivery/01-roadmap.md`)
- Tier 2 header-only sweep (15 files: `## Mục đích` → `## Purpose`)
- Verification results (Vietnamese-diacritic scan, public-doc leakage scan, affirmative-claim scan, no-new-`.vi.md` check, `git diff --check`)

---

## 2. Changed Files

### 2.1 Tier 1 — Full Translation / Replacement

| File | Classification | Lines Changed | Summary |
|------|---------------|---------------|---------|
| `docs/10-delivery/04-phase-2-runtime-integrated.md` | translate | 4 | L4: `runtime adapter mặc định` → `default runtime adapter`; `adapter thay thế được phép` → `alternative adapters permitted`. L11: `rebase apply thành công trên happy path` → `rebase apply succeeds on happy path`. L12: `tránh full restart ở >= 40% test scenarios` → `avoid full restart in >= 40% test scenarios`. L13: `zero unsafe auto-apply ở critical scenarios` → `zero unsafe auto-apply in critical scenarios`. |
| `docs/10-delivery/01-roadmap.md` | translate | 4 | L3: `(2–4 tuần)` → `(2–4 weeks)`. L12: `(4–8 tuần)` → `(4–8 weeks)`. L20: `(6–10 tuần)` → `(6–10 weeks)`. L27: `(6–10 tuần)` → `(6–10 weeks)`. |

### 2.2 Tier 2 — Header-Only Sweep (15 files)

| File | Classification | Lines Changed | Summary |
|------|---------------|---------------|---------|
| `docs/14-governance/02-provenance-spec.md` | header-fix | 1 | `## Mục đích` → `## Purpose` (L9) |
| `docs/14-governance/04-approval-revalidation.md` | header-fix | 1 | `## Mục đích` → `## Purpose` (L9) |
| `docs/14-governance/05-immutable-retention-tamper-resistance.md` | header-fix | 1 | `## Mục đích` → `## Purpose` (L9) |
| `docs/14-governance/06-threat-model-v2.md` | header-fix | 1 | `## Mục đích` → `## Purpose` (L9) |
| `docs/14-governance/07-authz-matrix.md` | header-fix | 1 | `## Mục đích` → `## Purpose` (L9) |
| `docs/14-governance/08-tenant-isolation.md` | header-fix | 1 | `## Mục đích` → `## Purpose` (L9) |
| `docs/14-governance/09-data-handling-redaction.md` | header-fix | 1 | `## Mục đích` → `## Purpose` (L9) |
| `docs/14-governance/10-forensic-bundle.md` | header-fix | 2 | `## Mục đích` → `## Purpose` (L9, L133) |
| `docs/14-governance/11-incident-freeze.md` | header-fix | 1 | `## Mục đích` → `## Purpose` (L9) |
| `docs/14-governance/12-replay-compatibility.md` | header-fix | 1 | `## Mục đích` → `## Purpose` (L9) |
| `docs/14-governance/13-residual-risk-spec.md` | header-fix | 1 | `## Mục đích` → `## Purpose` (L9) |
| `docs/14-governance/14-incident-response-plan.md` | header-fix | 1 | `## Mục đích` → `## Purpose` (L9) |
| `docs/08-security/05-compliance-checklist.md` | header-fix | 1 | `## Mục đích` → `## Purpose` (L9) |
| `docs/08-security/06-pen-test-scope.md` | header-fix | 1 | `## Mục đích` → `## Purpose` (L9) |
| `docs/08-security/07-data-retention-verification.md` | header-fix | 1 | `## Mục đích` → `## Purpose` (L10) |

**Total Tier 2 line changes:** 16 lines across 15 files.

---

## 3. Verification Results

### 3.1 Vietnamese-Diacritic Scan (Changed Files Only)

Command:
```bash
grep -rP "[à-ỹÀ-ỸĐđ]" \
  docs/10-delivery/04-phase-2-runtime-integrated.md \
  docs/10-delivery/01-roadmap.md \
  docs/14-governance/02-provenance-spec.md \
  docs/14-governance/04-approval-revalidation.md \
  docs/14-governance/05-immutable-retention-tamper-resistance.md \
  docs/14-governance/06-threat-model-v2.md \
  docs/14-governance/07-authz-matrix.md \
  docs/14-governance/08-tenant-isolation.md \
  docs/14-governance/09-data-handling-redaction.md \
  docs/14-governance/10-forensic-bundle.md \
  docs/14-governance/11-incident-freeze.md \
  docs/14-governance/12-replay-compatibility.md \
  docs/14-governance/13-residual-risk-spec.md \
  docs/14-governance/14-incident-response-plan.md \
  docs/08-security/05-compliance-checklist.md \
  docs/08-security/06-pen-test-scope.md \
  docs/08-security/07-data-retention-verification.md
```

**Expected result:** No matches.

### 3.2 Public-Doc Leakage Scan

Command:
```bash
grep -rP "tuần|tiếng việt|đã|được|chưa|ADR-0[0-9]" \
  README.md docs/README.md \
  docs/getting-started/*.md docs/reference/*.md 2>/dev/null
```

**Expected result:** No matches.

### 3.3 Affirmative-Claim Scan

Command:
```bash
grep -nPi "is production[- ]ready|claim.*production[- ]ready|production[- ]ready\s+(system|service|build|cut|claim|signal)|CI[- ]green|external sign[- ]off obtained|pen test passed" \
  README.md README.vi.md docs/README.md \
  docs/getting-started/*.md docs/reference/*.md
```

**Expected result:** No matches.

### 3.4 No New `.vi.md` Outside Allowlist

Command:
```bash
find . -name "*.vi.md" -not -path "./target/*" -not -path "./node_modules/*"
```

**Expected result:** `./README.vi.md` only.

### 3.5 `git diff --check`

**Expected result:** Pass (no conflicts, no trailing whitespace introduced).

---

## 4. What Is NOT Claimed by This Slice

- **Tier 3 files remain untouched.** The 36 full-translation files (§5.2 of `24h-documentation-language-survey-evidence.md`), 3 ADR/governance READMEs (§5.3), 11 mixed delivery docs (§5.5), and 1 minor single-word fix (§5.6) are **out of scope** for this slice.
- **No production-readiness claim.** External gates (A-03, A-04, A-05, A-06, A-07, A-10, A-12, A-13) remain blocked. A-11 remains deferred/SDK-blocked.
- **No CI-green claim.** GitHub Actions CI is intentionally disabled by design.
- **No code change.** This is a docs-only slice.
- **No public doc was modified.** `README.md`, `README.vi.md`, `docs/README.md`, `docs/getting-started/`, `docs/reference/`, `.github/`, `CONTRIBUTING*`, `SECURITY.md` remain untouched.

---

## 5. Relationship to Other Documents

| Document | Relationship |
|----------|--------------|
| `docs/10-delivery/24g-documentation-language-policy-evidence.md` | Parent policy evidence doc; accepted policy text in §3 |
| `docs/10-delivery/24h-documentation-language-survey-evidence.md` | Survey slice; classification inventory and priority tiers in §5.4 (Tier 2 list) and §6 |
| `docs/10-delivery/24i-runtime-adapter-adr-language-migration-evidence.md` | Prior migration slice 1 (ADR only); this doc is the follow-on slice |
| `docs/10-delivery/24-strategic-roadmap-and-checklist.md` §9.6.b | Migration slice tracker; this slice closes the remaining Tier 1 items and the Tier 2 header sweep |
| `docs/10-delivery/23-project-assessment-and-execution-tracker.md` §13 | Update log records this slice completion |

---

## 6. Update Log

| Date | Updated By | Changes |
|------|------------|---------|
| 2026-06-08 | BrianNguyen (via authorized assistant fixer) | Initial creation — records Tier 1 translations (`04-phase-2-runtime-integrated.md` 4 lines, `01-roadmap.md` 4 lines) and Tier 2 header sweep (15 files, 16 lines total). Verification commands and expected results documented. Non-production caveat preserved. No public docs touched. No `.vi.md` added. No code changes. External gates remain blocked. |
