# Production Infrastructure Scaffold (GCP + Terraform)

> **⚠️ CORE SCAFFOLD APPLIED + INTERNAL SMOKE DEPLOY — not production-ready.**
>
> This directory contains an **applied GCP infrastructure scaffold** and an **internal GKE smoke deployment** for the Intent Rebase Engine. Core resources (VPC, GKE, GCS, Cloud SQL) have been provisioned, and the `intent-api` container is running in the `intent-rebase` namespace with SQL-backed health/ready endpoints responding. The system is **not production-ready**: no real Slack/SMTP credentials are configured, no PITR restore has been tested, no load test has been executed against the provisioned infrastructure, no pen test has been conducted, and the deployment targets a single-node cluster without HPA or rolling-update resilience. All gates (FIND-001, FIND-003, FIND-004, A-05, A-06) remain OPEN or BLOCKED until named external evidence is obtained.

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
| A-05 Production Infrastructure | 🟡 SCAFFOLD APPLIED | Core GCP scaffold applied (VPC, GKE, GCS, Cloud SQL); namespace created. Not production-ready: no app deployed, no real secrets, no remote Terraform state, no PITR test, no load test, no pen test. |
| FIND-001 Production telemetry / Alertmanager real receivers | 🔴 OPEN | Alertmanager ConfigMap + `alertmanager-prod.yml` placeholders exist; Slack/SMTP not configured |
| FIND-003 Secret rotation / Vault or AWS SM | 🔴 OPEN | Kubernetes Secrets placeholder only; secret manager not deployed |
| FIND-004 Backup/restore PITR not validated | 🔴 OPEN | Cloud SQL backup + PITR configured in Terraform; not executed or validated |
| A-07 Pen Test | 🔴 NOT APPROVED | No change; internal review only |
| A-06 Load Testing (L3–L5) | 🔴 BLOCKED | Infrastructure exists (GKE + Cloud SQL) but no load test executed against it; no real Alertmanager receivers to validate alert firing under load |
| A-10 DLQ/NATS Production-Grade | 🔴 BLOCKED | Requires production NATS topology + SRE sign-off |
| A-12 Webhook Production Hardening | 🔴 BLOCKED | Requires secret manager + real delivery evidence |
| A-13 Forensic Immutable Storage | 🔴 BLOCKED | GCS retention policy is not S3 Object Lock; multi-cloud design may be needed |

## Forbidden Claims

| Forbidden Claim | Why It Is Forbidden Here |
|----------------|--------------------------|
| `Production-ready` | Core GCP scaffold applied + internal smoke deploy (app running on GKE with SQL-backed health/ready). Not production-ready: no real Slack/SMTP secrets, no PITR/pen/load test validated, no HPA, single-node cluster, no remote state backend migration, Recreate strategy |
| `FIND-001 RESOLVED` | Alertmanager receivers are placeholders; no real Slack/SMTP configured |
| `FIND-003 RESOLVED` | No secret manager deployed; Kubernetes Secrets are a placeholder |
| `FIND-004 RESOLVED` | Cloud SQL backups are Terraform config only; no restore validated |
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
│   ├── namespace.yaml                 # intent-rebase namespace
│   ├── deployment.yaml                # intent-api Deployment (internal smoke only)
│   ├── service.yaml                   # ClusterIP Service for intent-api
│   ├── migration-job.yaml             # Raw SQL migration Job (psql loop)
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
- No PITR restore test has been executed against Cloud SQL.
- No load test has been run against the provisioned infrastructure.
- No penetration test has been conducted.
- Terraform state migrated to GCS remote backend (`ire-tfstate-ferrum-497801`).

---

## Applied App Smoke Status

> **Date:** 2026-06-18
> **Image:** `us-central1-docker.pkg.dev/ferrum-497801/intent-rebase/intent-api:c166e57` (digest `sha256:7eb8299cf5d6cb881360a43d65fc18e21f602cf9112b150cccb4e76b6c5b32e6`)
> **Artifact Registry:** `us-central1-docker.pkg.dev/ferrum-497801/intent-rebase`

The following internal smoke deployment steps were executed against the provisioned scaffold:

| Step | Status | Details |
|------|--------|---------|
| GCS remote backend bucket created | ✅ Done | `ire-tfstate-ferrum-497801` for Terraform state |
| Terraform state migrated to GCS backend | ✅ Done | `backend.tf` initialized; local state no longer primary |
| DB password rotated via Terraform | ✅ Done | Supplied via `TF_VAR_db_password`; actual value stored outside repo |
| Artifact Registry repository created | ✅ Done | `intent-rebase` in `us-central1` |
| Docker image built and pushed | ✅ Done | `intent-api:c166e57` pushed to Artifact Registry |
| K8s Secret `app-secrets` applied | ✅ Done | Out-of-band; contains dummy JWT/API/HMAC and real DB URL |
| ConfigMap `intent-rebase-migrations` created | ✅ Done | Out-of-band from `infrastructure/migrations` |
| Migration Job `intent-rebase-migrations` completed | ✅ Done | `1/1` succeeded; raw `psql` loop over `.sql` files |
| Deployment `intent-api` applied | ✅ Done | Recreate strategy on single-node cluster; SQL-backed router initialized |
| Service `intent-api` applied | ✅ Done | ClusterIP `34.118.227.210`, port 8080 |
| Image pull permission fixed | ✅ Done | `roles/artifactregistry.reader` granted to GKE node SA after initial pull failure |
| Smoke checks from pod | ✅ Done | `/health` returned `{"status":"ok","uptime_seconds":146}`; `/ready` returned `{"status":"ready","uptime_seconds":0}` |

**Known caveats from this smoke deploy:**
- Single-node cluster: a second pod was created during an unnecessary `rollout restart` and went pending due to CPU exhaustion; rollout was undone and one pod remains running.
- Deployment uses `strategy: Recreate` to avoid two pods simultaneously on the single node. RollingUpdate with proper `maxSurge`/`maxUnavailable` must be configured before production.
- Resource requests are modest (`100m` CPU, `128Mi` memory). HPA and proper limits must be set before production.
- Migration Job uses raw `psql` and does **not** populate `sqlx` metadata tables (`_sqlx_migrations`). If the application relies on sqlx runtime verify, use `sqlx migrate run` or a `sqlx-cli` sidecar instead.
- No real Alertmanager receivers (Slack/SMTP) are configured.
- This is an **internal smoke deploy only**, not production-ready.

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
