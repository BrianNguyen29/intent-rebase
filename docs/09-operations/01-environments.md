# Environments

## Required environments
- local
- dev shared
- ephemeral preview
- staging
- pre-prod
- production

## Environment rules
- configs as code
- secrets from vault
- no shared prod credentials
- synthetic test tenants in non-prod
- replay testing in pre-prod for risky changes

## Promotion flow
dev -> preview -> staging -> pre-prod -> prod

## Staging scaffold status

**Location:** `infrastructure/staging/docker-compose.yml`

| Field | Value |
|-------|-------|
| **Scaffold exists** | ✅ Yes (`infrastructure/staging/`) |
| **Production-ready** | ❌ No — requires external SRE/security/load/pen gates |
| **Evidence strength** | Local docker-compose (staging-like) — NOT production-equivalent |
| **Last Updated** | April 2026 |

### Staging environment gates (not yet passed)

External gates required before production consideration:

- [ ] External SRE sign-off
- [ ] External security review / pen test
  - [ ] External load testing (L3+)
  - [ ] Compliance checklist completion

**A-07 Penetration Test staging requirement:** A-07 requires an **isolated staging environment** (separate GCP project or isolated VPC) with **synthetic data only** — no production credentials, no production customer data, no live API keys. The staging environment must be provisioned independently from the live production project `ferrum-497801` and destroyed after testing completes. See `docs/08-security/06-pen-test-scope.md` §Execution Readiness Addendum for the full prerequisite checklist.

**Note:** Do not claim `infrastructure/local/docker-compose.yml` as staging. The local stack is for local development only. Use `infrastructure/staging/docker-compose.yml` for staging-like evidence collection.

## Production scaffold status

**Location:** `infrastructure/production/`

| Field | Value |
|-------|-------|
| **Scaffold exists** | ✅ Yes (GCP + Terraform, Kubernetes Secrets, Alertmanager templates) |
| **Core infrastructure applied** | ✅ Yes (VPC, GKE, GCS, Cloud SQL Postgres on project `ferrum-497801`) |
| **Internal smoke deploy** | ✅ Yes (`intent-api` pod running in `intent-rebase` namespace; health/ready responding) |
| **Production-ready** | ❌ No — internal smoke deploy only; requires HPA, PDB, ingress/TLS, secret manager, real Alertmanager receivers, NATS, S3, PITR restore test, load test, pen test, external gates |
| **Evidence strength** | Internal smoke deploy against provisioned GCP resources; NOT production-equivalent |
| **Last Updated** | June 2026 |

**Note:** This scaffold is applied for A-05 but remains not production-ready. Terraform state migrated to GCS backend (`backend.tf`). All gates (FIND-001, FIND-003, FIND-004, A-05, A-06) remain OPEN or BLOCKED until validated with named external evidence. Do not apply Terraform or commit real secrets. The `intent-api` Deployment uses `strategy: Recreate` on a single-node cluster for smoke validation only.
