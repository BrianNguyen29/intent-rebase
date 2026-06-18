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

**A-07 staging deployed (2026-06-18):** Staging clone `a07-staging-postgres-20260618143121` created from primary `production-template-postgres-ed2c5bdd`, operation DONE, RUNNABLE, private IP `10.249.0.11`. Namespace `intent-rebase-staging` created with labels `env=staging,purpose=a07-pen-test`. Staging DB user password rotated to separate credential. Staging K8s Secret `app-secrets` created out-of-band with staging DB URL and generated JWT/API/HMAC. Staging Deployment `intent-api-6c4dd58fd4-h2mbs` and Service ClusterIP `34.118.231.110` applied with image `intent-api:9e26aaa`. Staging smoke `/health` ok uptime 22, `/ready` ready. ZAP baseline self-scan prep completed (initial: `FAIL-NEW: 0`, `WARN-NEW: 2`, `PASS: 65`; re-run after 401 header hardening: `FAIL-NEW: 0`, `WARN-NEW: 1`, `PASS: 66`). **A-07 remains OPEN** — self-scan is prep only; external tester still required.

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

**Note:** This scaffold is applied for A-05 but remains not production-ready. Terraform state migrated to GCS backend (`backend.tf`). All gates (FIND-001, FIND-003, FIND-004, A-05, A-06) remain OPEN or BLOCKED until validated with named external evidence. Do not apply Terraform or commit real secrets. The `intent-api` Deployment uses `strategy: RollingUpdate` with `maxSurge:1,maxUnavailable:0` on a node pool scaled to 2; HPA and PDB are applied. Phase 1/2/3/4 hardening is deployed but the system is not production-ready (A-07 open, no public ingress, ESO auto-sync not installed, full 30-min business-path load test not done, A-03/A-04 re-signoff not obtained).
