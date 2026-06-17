# Production Infrastructure Scaffold (GCP + Terraform)

> **⚠️ TEMPLATE ONLY — not applied; do not claim production-ready.**
>
> This directory contains a **non-applied production scaffold** for the Intent Rebase Engine on GCP. It is execution-prep for A-05 and related findings (FIND-001, FIND-003, FIND-004). All gates remain OPEN until real GCP credentials are provided, Terraform is applied, and named external evidence is obtained.

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
| A-05 Production Infrastructure | 🔴 BLOCKED | Scaffold is template-only; not applied |
| FIND-001 Production telemetry / Alertmanager real receivers | 🔴 OPEN | Alertmanager ConfigMap + `alertmanager-prod.yml` placeholders exist; Slack/SMTP not configured |
| FIND-003 Secret rotation / Vault or AWS SM | 🔴 OPEN | Kubernetes Secrets placeholder only; secret manager not deployed |
| FIND-004 Backup/restore PITR not validated | 🔴 OPEN | Cloud SQL backup + PITR configured in Terraform; not executed or validated |
| A-07 Pen Test | 🔴 NOT APPROVED | No change; internal review only |
| A-06 Load Testing (L3–L5) | 🔴 BLOCKED | No production infrastructure to test against |
| A-10 DLQ/NATS Production-Grade | 🔴 BLOCKED | Requires production NATS topology + SRE sign-off |
| A-12 Webhook Production Hardening | 🔴 BLOCKED | Requires secret manager + real delivery evidence |
| A-13 Forensic Immutable Storage | 🔴 BLOCKED | GCS retention policy is not S3 Object Lock; multi-cloud design may be needed |

## Forbidden Claims

| Forbidden Claim | Why It Is Forbidden Here |
|----------------|--------------------------|
| `Production-ready` | Scaffold is template-only; no infrastructure provisioned |
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
│   ├── secrets/
│   │   └── app-secrets.example.yaml   # Kubernetes Secret placeholders (do not commit real values)
│   └── configmaps/
│       └── alertmanager-config.yaml   # Alertmanager ConfigMap with Slack + SMTP placeholders
└── alertmanager/
    └── alertmanager-prod.yml          # Standalone Alertmanager YAML with Slack + SMTP placeholders
```

## Notes

- **GCS retention policy** is not equivalent to S3 Object Lock compliance mode. If immutable storage (A-13) requires strict Object Lock, consider a multi-cloud design or S3-compatible storage on GCP.
- **Deletion protection** is enabled on Cloud SQL and GKE resources in Terraform to prevent accidental destruction.
- **Private IP** is configured for Cloud SQL; public IP is disabled.
- **Kubernetes Secrets** are used as a placeholder secret mechanism. A production deployment should migrate to Vault, Google Secret Manager, or AWS Secrets Manager before any production claim.
- **No Terraform state backend** is configured in this scaffold. Add a GCS-backed state bucket before any apply in a team environment.

## Last Updated

2026-06-17
