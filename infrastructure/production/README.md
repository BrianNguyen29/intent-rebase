# Production Infrastructure Scaffold (GCP + Terraform)

> **⚠️ CORE SCAFFOLD APPLIED + INTERNAL SMOKE DEPLOY + LIVE BASELINE REPAIRED + ILB APPLIED — not production-ready.**
>
> This directory contains an **applied GCP infrastructure scaffold**, an **internal GKE smoke deployment**, a **live Cloud SQL baseline repair**, and an **applied Internal LoadBalancer** for the Intent Rebase Engine. Core resources (VPC, GKE, GCS, Cloud SQL) have been provisioned, the `intent-api` container is running in the `intent-rebase` namespace with SQL-backed health/ready endpoints responding, the live database `_sqlx_migrations` metadata has been repaired and verified, and an internal LoadBalancer provides private VPC-only access. The system is **not production-ready**: no real Slack/SMTP credentials are configured, PITR clone-only validated (2026-06-18) but RPO/RTO not measured against live production traffic and full DR program maturity remains open, no load test has been executed against the provisioned infrastructure, A-07 pen test staging is deployed and ZAP self-scan prep completed (0 FAIL, 2 WARN) but A-07 remains OPEN pending external engagement, and the deployment targets a single-node cluster without HPA or rolling-update resilience. All gates (FIND-001, FIND-003, FIND-004, A-05, A-06, A-07) remain OPEN or BLOCKED until named external evidence is obtained.

---

## Prerequisites

Before using this scaffold:

1. **GCP project** with billing enabled (replace `CHANGE_ME_GCP_PROJECT` everywhere)
2. **gcloud CLI** authenticated with appropriate IAM permissions
3. **Terraform >= 1.5.0** installed
4. **kubectl** configured (after GKE cluster is provisioned)
5. **External review gates** (A-03 SRE, A-04 Security) closed or revisited with named evidence
6. **Real secrets** generated and stored in a production secret manager (this scaffold uses Kubernetes Secrets as a placeholder mechanism)

## Execution Order

1. **Copy and fill `.env.example`** — replace all `CHANGE_ME_*` placeholders with real values (do not commit `.env`)
2. **Terraform init / plan** (do **not** apply yet):
   ```bash
   cd infrastructure/production/terraform
   terraform init
   terraform plan
   ```
3. **Review the plan** with SRE/security before any apply
4. **Apply only after** external gates are revisited and budget is approved
5. **Deploy Kubernetes manifests** after the GKE cluster is operational:
   ```bash
   kubectl apply -f infrastructure/production/kubernetes/namespace.yaml
   kubectl apply -f infrastructure/production/kubernetes/secrets/app-secrets.example.yaml   # after replacing placeholders
   kubectl apply -f infrastructure/production/kubernetes/configmaps/alertmanager-config.yaml
   ```

## Blocked Gates Mapping

| Gate / Finding | Status | Scaffold Role |
|----------------|--------|-------------|
| A-05 Production Infrastructure | 🟡 SCAFFOLD APPLIED + INTERNAL SMOKE DEPLOY | Core GCP scaffold applied (VPC, GKE, GCS, Cloud SQL); namespace created; `intent-api` pod running with health/ready responding. Not production-ready: no HPA, no ingress, no real secrets, no remote Terraform state, PITR clone-only validated, no load test, no pen test; full DR maturity/RPO-RTO still open. |
| FIND-001 Production telemetry / Alertmanager real receivers | 🔴 OPEN | Alertmanager ConfigMap + `alertmanager-prod.yml` placeholders exist; Slack/SMTP not configured |
| FIND-003 Secret rotation / Vault or AWS SM | 🔴 OPEN | Kubernetes Secrets placeholder only; secret manager not deployed |
| FIND-004 Backup/restore PITR clone-only validated | 🟡 VALIDATED — CLONE-ONLY | Cloud SQL PITR clone restore validated on 2026-06-18 against separate clone `pitr-restore-test-20260618084607`; RPO/RTO not measured against live production traffic; full DR program maturity remains open |
| A-07 Pen Test | 🔴 NOT APPROVED | No change; internal review only |
| A-06 Load Testing (L3–L5) | 🔴 BLOCKED | Infrastructure exists (GKE + Cloud SQL) but no load test executed against it; no real Alertmanager receivers to validate alert firing under load |
| A-10 DLQ/NATS Production-Grade | 🔴 BLOCKED | Requires production NATS topology + SRE sign-off |
| A-12 Webhook Production Hardening | 🔴 BLOCKED | Requires secret manager + real delivery evidence |
| A-13 Forensic Immutable Storage | 🔴 BLOCKED | GCS retention policy is not S3 Object Lock; multi-cloud design may be needed |

## Forbidden Claims

| Forbidden Claim | Why It Is Forbidden Here |
|----------------|--------------------------|
| `Production-ready` | Core GCP scaffold applied + internal smoke deploy (app running on GKE with SQL-backed health/ready). Not production-ready: no real Slack/SMTP secrets, no full DR/live RPO-RTO, pen, or load test validated, no HPA, single-node cluster, no remote state backend migration, Recreate strategy |
| `FIND-001 RESOLVED` | Alertmanager receivers are placeholders; no real Slack/SMTP configured |
| `FIND-003 RESOLVED` | No secret manager deployed; Kubernetes Secrets are a placeholder |
| `FIND-004 RESOLVED` | Cloud SQL PITR clone-only validated (2026-06-18), but full DR/live RPO-RTO not validated; use only VALIDATED — CLONE-ONLY |
| `FIND-005 RESOLVED` | No pen test executed; scope remains planning-only |
| `External sign-off obtained` | A-03/A-04 are APPROVED WITH CONDITIONS; A-07 is NOT APPROVED |
| `CI-green` | No CI changes; local gates remain source of truth |

## Directory Layout

```text
infrastructure/production/
├── README.md                          # This file
├── .env.example                       # Placeholder environment variables (no secrets)
├── terraform/
│   ├── versions.tf                    # Provider versions (google ~> 6.x, kubernetes ~> 3.x)
│   ├── variables.tf                   # Input variables (all default to CHANGE_ME_*)
│   ├── main.tf                        # Provider configuration
│   ├── network.tf                     # VPC + subnet + private IP allocation
│   ├── postgres.tf                    # Cloud SQL Postgres (backups, PITR, private IP)
│   ├── gke.tf                         # GKE cluster + node pool + service account
│   ├── storage.tf                     # GCS bucket (retention, uniform access, no Object Lock)
│   └── outputs.tf                     # Terraform outputs
├── kubernetes/
│   ├── internal-load-balancer-service.yaml # GKE Internal LoadBalancer (NOT APPLIED — manifest-only; private-only, no public IP)
│   ├── deployment.yaml                # intent-api Deployment (internal smoke only)
│   ├── horizontal-pod-autoscaler.yaml  # HPA for intent-api (NOT APPLIED — manifest-only; needs capacity)
│   ├── migration-job.yaml             # sqlx migration Job using intent-api image (INTENT_API_RUN_MIGRATIONS=true)
│   ├── pod-disruption-budget.yaml      # PDB for intent-api (NOT APPLIED — manifest-only; needs capacity)
│   ├── service.yaml                   # ClusterIP Service for intent-api
│   ├── secrets/
│   │   └── app-secrets.example.yaml   # Kubernetes Secret placeholders (do not commit real values)
│   └── configmaps/
│       └── alertmanager-config.yaml   # Alertmanager ConfigMap with Slack + SMTP placeholders
└── alertmanager/
    └── alertmanager-prod.yml          # Standalone Alertmanager YAML with Slack + SMTP placeholders
```

## Notes

- **GCS retention policy** is not equivalent to S3 Object Lock compliance mode. If immutable storage (A-13) requires strict Object Lock, consider a multi-cloud design or S3-compatible storage on GCP.
- **Deletion protection** is enabled on Cloud SQL in Terraform to prevent accidental destruction. GKE cluster has `deletion_protection = false` for this scaffold/test apply and must be re-enabled before any production claim.
- **Private IP** is configured for Cloud SQL; public IP is disabled.
- **Kubernetes Secrets** are used as a placeholder secret mechanism. A production deployment should migrate to Vault, Google Secret Manager, or AWS Secrets Manager before any production claim.
- **HPA + PDB manifests** are added under `kubernetes/` but are **not applied** to the live single-node cluster. The current node pool has insufficient capacity for HPA to scale meaningfully or for PDB `minAvailable: 1` to be honored during drains. Apply only after scaling node pool capacity and switching `strategy: Recreate` to `RollingUpdate` with `maxSurge: 0, maxUnavailable: 1` (or a larger cluster). HPA is safe to create with `Recreate` but scaling events will recreate the pod (brief downtime).
- **Migration standardization**: `INTENT_API_RUN_MIGRATIONS=true` migration-only mode added to `intent-api` binary. K8s Job `migration-job.yaml` updated to use the `intent-api:733e1ca` image (includes migration-mode support). **Live Deployment rolled out** on 2026-06-18 with pod `intent-api-6985547786-27hxd` running; `/health` and `/ready` endpoints responding; JWT guard passed; SQL-backed router initialized. This is an internal smoke deploy, not production-ready. Migrations are embedded at compile time via `sqlx::migrate!`; the image must be rebuilt when migration files change. **Existing live DB `_sqlx_migrations` metadata gap remains** — the database was raw-psql migrated and needs a baseline/repair step or recreation before `_sqlx_migrations` is populated. The standardized Job is for future clean deploys after the live DB gap is resolved.
- **No Terraform state backend** is configured in this scaffold. Local state exists and is gitignored; migrate to a GCS-backed state bucket before any team use or further apply. **A GCS backend (`backend.tf`) is now present; initialize with `terraform init` to migrate state.**
- **Cloud SQL password** is supplied via the `TF_VAR_db_password` environment variable. Do not commit a default value or a `.tfvars` file containing secrets.

## Applied Scaffold Status

> **Date:** 2026-06-18
> **GCP Project:** `ferrum-497801`
> **Account:** `nhduong020301@gmail.com`

Terraform apply **completed** for the core GCP scaffold. This is **infrastructure scaffold applied + dry-run evidence**, not full production readiness. The following resources were created:

| Category | Resources |
|----------|-----------|
| Network | VPC (`production-template-vpc`), subnet, private IP allocation, private services connection |
| GKE | Zonal cluster `production-template-gke` in `us-central1-a`, node pool, GKE service account |
| Storage | GCS bucket `ire-prod-ferrum-497801-production-template-ed2c5bdd` (retention policy, uniform access, versioning) |
| Database | Cloud SQL Postgres `production-template-postgres-ed2c5bdd` (Enterprise edition, private IP `10.249.0.3`, backups, PITR), database `intent_rebase`, user `intent_rebase_app` |

**Terraform outputs:**
- `gcs_bucket_name` = `ire-prod-ferrum-497801-production-template-ed2c5bdd`
- `gke_cluster_name` = `production-template-gke`
- `gke_cluster_location` = `us-central1-a`
- `postgres_instance_name` = `production-template-postgres-ed2c5bdd`
- `postgres_private_ip` = `10.249.0.3`
- `vpc_id` = `projects/ferrum-497801/global/networks/production-template-vpc`

**Kubernetes:**
- `kubectl` context configured for `production-template-gke` via `gcloud` access token (gke-gcloud-auth-plugin unavailable in this environment).
- Namespace `intent-rebase` created via `kubectl apply -f infrastructure/production/kubernetes/namespace.yaml`.
- Secret `app-secrets` applied out-of-band with dummy JWT/API/HMAC and real DB URL (do not commit real values).
- ConfigMap `alertmanager-config` passed **server dry-run only** (`kubectl apply --dry-run=server`); not applied to the cluster (Slack/SMTP placeholders only).

**Remaining blockers (not resolved by infrastructure scaffold apply):**
- No real Slack/SMTP credentials configured in Alertmanager.
- No load test has been run against the provisioned infrastructure.
- No penetration test has been conducted.
- Terraform state migrated to GCS remote backend (`ire-tfstate-ferrum-497801`).
- PITR restore validated against a separate Cloud SQL clone (2026-06-18); RPO/RTO not measured against live production traffic. Full DR program maturity remains open.

---

## Applied App Smoke Status

> **Date:** 2026-06-18
> **Image (smoke deploy):** `us-central1-docker.pkg.dev/ferrum-497801/intent-rebase/intent-api:c166e57` (digest `sha256:7eb8299cf5d6cb881360a43d65fc18e21f602cf9112b150cccb4e76b6c5b32e6`)
> **Image (migration-mode build):** `us-central1-docker.pkg.dev/ferrum-497801/intent-rebase/intent-api:733e1ca` (digest `sha256:5b125abeff71c8c575ac8d7b2708f01e37e1f2307f94d97d287d90469661abb7`) — includes `INTENT_API_RUN_MIGRATIONS=true` migration-only mode support
> **Artifact Registry:** `us-central1-docker.pkg.dev/ferrum-497801/intent-rebase`

The following internal smoke deployment steps were executed against the provisioned scaffold:

| Step | Status | Details |
|------|--------|---------|
| GCS remote backend bucket created | ✅ Done | `ire-tfstate-ferrum-497801` for Terraform state |
| Terraform state migrated to GCS backend | ✅ Done | `backend.tf` initialized; local state no longer primary |
| DB password rotated via Terraform | ✅ Done | Supplied via `TF_VAR_db_password`; actual value stored outside repo |
| Artifact Registry repository created | ✅ Done | `intent-rebase` in `us-central1` |
| Docker image built and pushed | ✅ Done | `intent-api:c166e57` pushed to Artifact Registry (smoke deploy image) |
| Docker image rebuilt with migration-mode | ✅ Done | `intent-api:733e1ca` pushed to Artifact Registry (includes `INTENT_API_RUN_MIGRATIONS=true` support) |
| K8s Secret `app-secrets` applied | ✅ Done | Out-of-band; contains dummy JWT/API/HMAC and real DB URL |
| ConfigMap `intent-rebase-migrations` created | ✅ Done | Out-of-band from `infrastructure/migrations` |
| Migration Job `intent-rebase-migrations` completed | ✅ Done | `1/1` succeeded; raw `psql` loop over `.sql` files |
| Deployment `intent-api` applied | ✅ Done | Recreate strategy on single-node cluster; SQL-backed router initialized |
| Service `intent-api` applied | ✅ Done | ClusterIP `34.118.227.210`, port 8080 |
| Image pull permission fixed | ✅ Done | `roles/artifactregistry.reader` granted to GKE node SA after initial pull failure |
| Smoke checks from pod | ✅ Done | `/health` returned `{"status":"ok","uptime_seconds":146}`; `/ready` returned `{"status":"ready","uptime_seconds":0}` |
| Live Deployment image rollout | ✅ Done | Applied `deployment.yaml` with image `intent-api:733e1ca`; pod `intent-api-6985547786-27hxd` running on node `gke-production-templ-production-templ-0abeaf6e-4g41`; IP `10.4.0.16`; `/health` and `/ready` endpoints responding; JWT guard passed, SQL-backed router initialized |
| Live DB `_sqlx_migrations` baseline repair | ✅ Done | On-demand backup `1781790576640` created on primary; prep clone `sqlx-live-baseline-prep-20260618135133` created; clean DB `intent_rebase_clean` migrated; baseline imported (`COPY 22`, `sqlx_migrations_count=22`); live primary DB verified (`relation "_sqlx_migrations" already exists, skipping`, `Migrations completed successfully`); final check `public_table_count=20`, `core_tables=_sqlx_migrations,graph_edges,graph_nodes,intent_versions,webhook_outbox`, `sqlx_migrations_count=22`, `failed_migrations=0`; prep clone deleted; app health `/health` ok uptime 6664; `/ready` ready |
| Internal LoadBalancer applied | ✅ Done | `internal-load-balancer-service.yaml` applied; Service `intent-api-internal-lb` type LoadBalancer, ClusterIP `34.118.233.220`, private IP `10.0.0.12`, port `8080:31731/TCP`; smoke from app pod via internal LB: `/health` ok uptime 7134, `/ready` ready |
| A-07 staging environment deployed | ✅ Done | Staging clone `a07-staging-postgres-20260618143121` created from primary, operation DONE, RUNNABLE, private IP `10.249.0.11`; namespace `intent-rebase-staging` created with labels `env=staging,purpose=a07-pen-test`; staging DB user password rotated to separate credential; staging K8s Secret `app-secrets` created out-of-band with staging DB URL and generated JWT/API/HMAC; staging Deployment `intent-api-66dcdc6d98-t9dzk` and Service ClusterIP `34.118.231.110` applied; image `intent-api:733e1ca`; `/health` ok uptime 16; `/ready` ready |
| ZAP baseline self-scan (A-07 prep) | ✅ Done | ZAP Docker image `ghcr.io/zaproxy/zaproxy:stable` via port-forward `0.0.0.0:18081` against staging target `http://host.docker.internal:18081`; report saved to `/tmp/opencode/zap-a07-staging-20260618b`; result: `FAIL-NEW: 0`, `WARN-NEW: 2`, `PASS: 65`, `ZAP_RC=2`; WARNs: `Content-Type Header Missing [10019] x3` and `Non-Storable Content [10049] x3` on unauthenticated 401 responses at `/`, `/robots.txt`, `/sitemap.xml`. **This is self-scan prep only; does NOT close A-07.** |

**Known caveats from this smoke deploy:**
- Single-node cluster: a second pod was created during an unnecessary `rollout restart` and went pending due to CPU exhaustion; rollout was undone and one pod remains running.
- Deployment uses `strategy: Recreate` to avoid two pods simultaneously on the single node. RollingUpdate with proper `maxSurge`/`maxUnavailable` must be configured before production.
- Resource requests are modest (`100m` CPU, `128Mi` memory). HPA and proper limits must be set before production.
- No real Alertmanager receivers (Slack/SMTP) are configured.
- A-07 staging environment (`intent-rebase-staging` namespace, staging clone `a07-staging-postgres-20260618143121`) persists and incurs cost until teardown.
- This is an **internal smoke deploy only**, not production-ready.

---

## Ingress Strategy

> **Decision:** Private-only (ClusterIP) for the current phase. Public ingress is explicitly gated.

| Phase | Target | Status | Prerequisites |
|-------|--------|--------|---------------|
| Current | `ClusterIP` (internal) only | ✅ Active | Intent-api pod reachable within `intent-rebase` namespace via `intent-api:8080` |
| Next incremental (if needed) | Internal LoadBalancer, VPN, or bastion host | ✅ APPLIED — PRIVATE ONLY | Service `intent-api-internal-lb` applied with GKE internal LB annotations (`networking.gke.io/load-balancer-type: Internal`). ClusterIP `34.118.233.220`, private IP `10.0.0.12`, port `8080:31731/TCP`. Smoke test from app pod confirmed `/health` and `/ready` responding via internal LB. Incurring GCP networking charges. Public ingress remains gated. |
| Public ingress | Public LoadBalancer / GKE Ingress with domain, TLS, WAF | 🔴 GATED | Requires: A-04 updated signoff (security review for external surface), A-07 completed and remediation verified, dedicated domain + TLS certificate management, Cloud Armor or WAF evaluation, `deletion_protection = true` on GKE, node pool ≥ 2 nodes, HPA + PDB applied, real Alertmanager receivers, and documented runbook. |

**Rationale:** The current `intent-api` has no consumer requiring public access. Exposing it prematurely increases attack surface without business justification. When public ingress is needed, the gated checklist above must be completed in order.

---

## Bootstrap Status

> **Date:** 2026-06-18
> **Account:** `nhduong020301@gmail.com`
> **GCP Project:** `ferrum-497801`

The following GCP pre-work was completed to enable Terraform apply. Terraform apply was subsequently executed and the core scaffold is now provisioned (see **Applied Scaffold Status** above).

| Step | Status | Details |
|------|--------|---------|
| Required APIs enabled | ✅ Done | `compute.googleapis.com`, `container.googleapis.com`, `iam.googleapis.com`, `servicenetworking.googleapis.com`, `sqladmin.googleapis.com`, `storage.googleapis.com` |
| Terraform service account created | ✅ Done | `intent-rebase-terraform@ferrum-497801.iam.gserviceaccount.com` |
| IAM roles bound to service account | ✅ Done | `roles/cloudsql.admin`, `roles/compute.networkAdmin`, `roles/container.admin`, `roles/iam.serviceAccountAdmin`, `roles/iam.serviceAccountUser`, `roles/storage.admin` |
| Terraform init/plan/apply | ✅ Done | Core scaffold applied (VPC, subnet, GKE, GCS, Cloud SQL). See Applied Scaffold Status for details. |
| Kubernetes namespace created | ✅ Done | `intent-rebase` namespace created via `kubectl apply` |
| Kubernetes secrets delivered | ✅ Done | `app-secrets` applied out-of-band with dummy JWT/API/HMAC and real DB URL; `alertmanager-config` still placeholder-only (Slack/SMTP not configured) |
| External review gates | 🔴 OPEN | FIND-001, FIND-003, FIND-004, A-05, A-06 remain open or blocked until validated with named external evidence and production hardening |

## Last Updated

2026-06-18

---

## Remaining Completion Items (Recommended Order)

This section mirrors the Phase 4 tracker in `docs/10-delivery/23-project-assessment-and-execution-tracker.md` §6. It is a concise checklist for moving from internal smoke deploy to production readiness.

| # | Item | Owner | Blocker / Prerequisite | Status |
|---|------|-------|------------------------|--------|
| 1 | **K8s hardening**: HPA + PDB manifests added under `kubernetes/` but NOT APPLIED to live cluster. Remaining: rolling-update (`maxSurge`/`maxUnavailable`), `deletion_protection = true` on GKE, ingress/TLS/domain, scale node pool before HPA/PDB apply | SRE / Backend Lead | Scaffold exists | 🟡 MANIFESTS ADDED — NOT APPLIED |
| 1a | **Internal LoadBalancer**: `internal-load-balancer-service.yaml` applied with GKE internal LB annotations (`networking.gke.io/load-balancer-type: Internal`, `cloud.google.com/load-balancer-type: Internal`). Service `intent-api-internal-lb` type LoadBalancer, ClusterIP `34.118.233.220`, private IP `10.0.0.12`, port `8080:31731/TCP`. Smoke from app pod confirmed `/health` and `/ready` responding via internal LB. Private-only (no public IP). Public ingress remains gated. Incurring GCP networking charges. | SRE / Backend Lead | A-05, internal consumer access need | ✅ APPLIED — PRIVATE ONLY |
| 2 | **Secret manager migration**: Replace K8s Secret placeholder with Vault / Google Secret Manager / AWS SM; validate key rotation grace window | SRE / Security | A-05, A-12 | 🔴 OPEN |
| 3 | **NATS + S3 on GCP**: Provision NATS with JetStream or Cloud Pub/Sub; configure S3-compatible storage or GCS Object Lock equivalent | SRE / Backend Lead | A-05, A-10, A-13 | 🔴 OPEN |
| 4 | **Monitoring + Alertmanager**: Configure real Slack/SMTP receivers; validate all alert types fire under sustained load | SRE / Backend Lead | A-05, A-06 | 🔴 OPEN |
| 5 | **Cloud SQL PITR restore test**: Execute documented PITR procedure against `production-template-postgres-ed2c5bdd`; measure RPO/RTO | SRE | A-05 | ✅ VALIDATED — CLONE-ONLY (2026-06-18). Clone `pitr-restore-test-20260618084607` created, validated (19 public tables, core tables present), deleted. RPO/RTO not measured against live production traffic. `_sqlx_migrations` absent due to raw-psql migrations (expected). Full DR program maturity remains open. |
| 6 | **Load test against provisioned infra**: Run 30min sustained + all alert types + real receivers on GKE + Cloud SQL | Backend Lead / SRE | A-05, A-03 | 🔴 OPEN |
| 7 | **External SRE sign-off (A-03)**: Named third-party evidence against hardened infrastructure | External SRE | Items 1–6 above | 🔴 OPEN |
| 8 | **Terraform state backend access validation**: GCS backend configured (`backend.tf`); routine access validation and recovery docs | SRE | A-05 scaffold exists | 🟡 DOCUMENTED |
| 9 | **CI/CD pipeline for GKE**: Build, push, deploy automation; no CI changes have been made | Backend Lead / SRE | A-05 | 🔴 OPEN |
| 10 | **Migration standardization**: `INTENT_API_RUN_MIGRATIONS=true` migration-only mode added to `intent-api` binary; K8s Job `migration-job.yaml` uses `intent-api:733e1ca` image (includes `INTENT_API_RUN_MIGRATIONS=true` support). **Live Deployment rolled out** with `intent-api:733e1ca` (pod `intent-api-6985547786-27hxd` on node `gke-production-templ-production-templ-0abeaf6e-4g41`, IP `10.4.0.16`); health/ready endpoints responding; JWT guard passed; SQL-backed router initialized. **Live DB `_sqlx_migrations` baseline repaired and verified** (2026-06-18): on-demand backup `1781790576640` created; prep clone `sqlx-live-baseline-prep-20260618135133` created; clean DB `intent_rebase_clean` migrated; baseline imported (`COPY 22`, `sqlx_migrations_count=22`); live primary DB verified (`relation "_sqlx_migrations" already exists, skipping`, `Migrations completed successfully`); final check `public_table_count=20`, `core_tables=_sqlx_migrations,graph_edges,graph_nodes,intent_versions,webhook_outbox`, `sqlx_migrations_count=22`, `failed_migrations=0`; prep clone deleted; app health `/health` ok uptime 6664; `/ready` ready. | Backend Lead / SRE | A-05 | ✅ APPLIED — LIVE MIGRATION-MODE + BASELINE REPAIRED |
| 11 | **Live DB sqlx metadata baseline/repair**: **Live primary DB baseline repaired and verified** (2026-06-18): on-demand backup `1781790576640` created on primary `production-template-postgres-ed2c5bdd`; prep clone `sqlx-live-baseline-prep-20260618135133` created, operation `DONE` with no error, RUNNABLE with private IP `10.249.0.9`; clean DB `intent_rebase_clean` created; migration-mode Job `sqlx-liveprep-clean-migrate-20260618135133` succeeded (`Migrations completed successfully`); baseline import Job `sqlx-live-baseline-import-20260618135133` succeeded (`COPY 22`, `sqlx_migrations_count=22`, `minmax=1..22`); live verify Job `sqlx-live-verify-20260618135133` succeeded against primary (`relation "_sqlx_migrations" already exists, skipping`, `Migrations completed successfully`); final check Job `sqlx-live-final-check-20260618135133` confirmed: `database=intent_rebase`, `public_table_count=20`, `core_tables=_sqlx_migrations,graph_edges,graph_nodes,intent_versions,webhook_outbox`, `sqlx_migrations_count=22`, `failed_migrations=0`; prep clone deleted; post-delete `BASELINE_PREP_CLONE_DELETED`; app health `/health` ok uptime 6664, `/ready` ready. | Backend Lead / SRE | A-05 | ✅ APPLIED — LIVE BASELINE REPAIRED + VERIFIED |
| 12 | **A-07 Penetration Test execution**: Staging environment deployed and ZAP self-scan prep completed (2026-06-18). Staging clone `a07-staging-postgres-20260618143121` created from primary, operation `DONE`, RUNNABLE, private IP `10.249.0.11`; namespace `intent-rebase-staging` created with labels `env=staging,purpose=a07-pen-test`; staging DB user password rotated to separate credential; staging K8s Secret `app-secrets` created out-of-band with staging DB URL and generated JWT/API/HMAC; staging Deployment `intent-api-66dcdc6d98-t9dzk` and Service ClusterIP `34.118.231.110` applied; image `intent-api:733e1ca`; staging smoke `/health` ok uptime 16, `/ready` ready. ZAP baseline self-scan: Docker image `ghcr.io/zaproxy/zaproxy:stable` via port-forward `0.0.0.0:18081` against staging target `http://host.docker.internal:18081`; report saved to `/tmp/opencode/zap-a07-staging-20260618b`; result: `FAIL-NEW: 0`, `WARN-NEW: 2`, `PASS: 65`, `ZAP_RC=2`; WARNs: `Content-Type Header Missing [10019] x3` and `Non-Storable Content [10049] x3` on unauthenticated 401 responses at `/`, `/robots.txt`, `/sitemap.xml`. **A-07 remains OPEN** — self-scan is prep only; external tester (HackerOne/Bugcrowd/freelance) still required with evidence checklist (PDF + JSON report, HIGH/CRITICAL remediation evidence, retest confirmation). | External Pen Test | A-04, staging env, separate credentials | 🟡 STAGING DEPLOYED + ZAP PREP DONE (0 FAIL, 2 WARN) — EXTERNAL TESTER STILL OPEN |

> **No overclaim:** The scaffold is applied and the app is running internally, but none of the hardening items above are complete. Do not claim production readiness until all items are closed with evidence.
