# 09 — Public Repo Security Audit

**Status:** `DOCUMENTED`
**Phase:** Phase 3 — Ops Evidence Track
**Owner:** Backend Lead (solo practitioner)
**Last Updated:** May 2026

---

## Purpose

This document records the scope, findings, and remediation policy for the **public-repo security scan** conducted as part of the Phase 3 evidence track. It is a point-in-time audit result, not a forensic guarantee.

> **⚠️ Scope Limitation**
>
> This audit covers targeted pattern scans of the current codebase and git history. It does not constitute a full third-party penetration test, automated secret scanning infrastructure deployment, or comprehensive forensic analysis. GitHub Advanced Security (Advanced Security secret scanning / push protection) is **not enabled** on this repository.

---

## Scan Scope

### Files Scanned

| Category | Files |
|----------|-------|
| Environment templates | `.env.example`, `infrastructure/staging/.env.example` |
| CI/CD workflows | `.github/workflows/ci.yml`, `.github/workflows/smoke.yml` |
| Docker compose | `infrastructure/local/docker-compose.yml` |
| Documentation | `docs/09-operations/08-secrets-inventory.md`, `docs/09-operations/02-ci-cd.md` |

### Patterns Searched

**High-confidence secret patterns (no matches found):**

| Pattern | Description | Matches |
|---------|-------------|---------|
| `AKIA*` | AWS access key ID prefix | 0 |
| `ASIA*` | AWS temporary access key prefix | 0 |
| `ghp_*` | GitHub personal access token | 0 |
| `github_pat_*` | GitHub fine-grained PAT | 0 |
| `BEGIN PRIVATE KEY` | Private key blocks | 0 |
| `xox[baprs]-[0-9]{10,}` | Slack tokens | 0 |
| `sk-[A-Za-z0-9]{32,}` | OpenAI API keys | 0 |
| `AIza[A-Za-z0-9]{35}` | Google API keys | 0 |

**Git history high-confidence patterns (no matches found):**

| Pattern | Description | Matches |
|---------|-------------|---------|
| `AKIA*` | AWS access keys in history | 0 |
| `ghp_*` | GitHub PATs in history | 0 |
| `github_pat_*` | GitHub fine-grained PATs in history | 0 |
| `BEGIN PRIVATE KEY` | Private keys in history | 0 |

**Secret-like assignment scan (findings — all dev placeholders):**

| File | Finding | Classification |
|------|---------|-----------------|
| `.github/workflows/ci.yml` | `POSTGRES_PASSWORD: intent_rebase_dev` | Dev-only placeholder; not a real credential |
| `infrastructure/local/docker-compose.yml` | `POSTGRES_PASSWORD: intent_rebase_dev` | Dev-only placeholder; not a real credential |
| `infrastructure/local/docker-compose.yml` | `MINIO_ROOT_USER: minioadmin` | MinIO default; not a shared secret |
| `infrastructure/local/docker-compose.yml` | `MINIO_ROOT_PASSWORD: minioadmin` | MinIO default; not a shared secret |
| `.env.example` | `JWT_SECRET=your-strong-secret-here-min-32-chars` | Explicit placeholder; user-directed to replace |
| `.env.example` | `AWS_ACCESS_KEY_ID=minioadmin` (prior) | Hardened to `CHANGE_ME_*` placeholder |
| `.env.example` | `AWS_SECRET_ACCESS_KEY=minioadmin` (prior) | Hardened to `CHANGE_ME_*` placeholder |
| `infrastructure/staging/.env.example` | `STAGING_POSTGRES_PASSWORD=CHANGE_ME_staging_*` | Explicit `CHANGE_ME_` placeholder |
| `infrastructure/staging/.env.example` | `STAGING_MINIO_ROOT_PASSWORD=CHANGE_ME_staging_*` | Explicit `CHANGE_ME_` placeholder |

---

## GitHub Advanced Security

**Status:** GitHub Advanced Security is **not enabled** on this repository.

GitHub Advanced Security provides:
- **Secret scanning** — detects secrets in code and alerts via PR comments
- **Push protection** — blocks commits containing detected secrets before they enter history

### Recommendation

If the repository plan supports it, enable **GitHub Advanced Security** to get automated secret scanning:

1. Go to repository **Settings** → **Security and analysis**
2. Enable **GitHub Advanced Security**
3. Enable **Secret scanning** and **Push protection**

For open-source repositories, GitHub Advanced Security features are available on public repos at no cost. For private repos, they require a paid plan (GitHub Enterprise).

---

## Remediation Policy

If a real secret is discovered in the repository at any point:

### Immediate Steps

1. **Do not assume the secret is safe** even if it appears to be a dev placeholder
2. **Rotate the secret immediately** if it was a real credential (AWS keys, GitHub tokens, database passwords, etc.)
3. **Audit access logs** for the affected service (AWS CloudTrail, GitHub audit log, database logs, etc.) to determine if the secret was used by an unauthorized party
4. **Revoke and reissue** the credential through proper channels
5. **Document the incident** in this document and notify affected parties

### Contact

For responsible disclosure of any secrets found in this repository: **[repository owner/maintainer]**

---

## Findings Summary

| Finding | Severity | Status |
|---------|----------|--------|
| No high-confidence secrets (AWS keys, GitHub PATs, private keys, Slack/OpenAI/Google keys) found in current code or git history | N/A | ✅ Clear |
| `.env.example` previously contained `minioadmin` as MinIO credentials | Low → Fixed | ✅ Hardened to `CHANGE_ME_*` placeholders |
| `docker-compose.yml` contains `minioadmin` and `intent_rebase_dev` as local dev defaults | Low | ✅ Documented as local-only; not removable without breaking local dev ergonomics |
| CI workflow contains `intent_rebase_dev` as Postgres password for integration test | Low | ✅ Documented as local-only test fixture; not a real credential |
| Staging `.env.example` uses explicit `CHANGE_ME_*` placeholders throughout | N/A | ✅ Compliant |
| GitHub Advanced Security not enabled | Medium | ⚠️ Recommend enabling if plan supports |

---

## Related Documents

| Document | Relationship |
|----------|--------------|
| `docs/09-operations/08-secrets-inventory.md` | Secrets inventory and rotation procedure templates |
| `docs/09-operations/02-ci-cd.md` | CI/CD free-safe policy and workflow documentation |
| `docs/09-operations/01-environments.md` | Environment definitions (local, staging, production) |

---

## Update Log

| Date | Updated By | Changes |
|------|------------|---------|
| May 2026 | (fixer) | Initial creation — public repo security audit; scan scope and findings; GitHub Advanced Security status and recommendation; remediation policy |
