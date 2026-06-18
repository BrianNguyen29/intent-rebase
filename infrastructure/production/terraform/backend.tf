# TEMPLATE ONLY — not applied; do not claim production-ready.
# Remote Terraform state backend (GCS).
# NOTE: State may contain sensitive values; bucket access must be tightly controlled.

terraform {
  backend "gcs" {
    bucket = "ire-tfstate-ferrum-497801"
    prefix = "intent-rebase/production-template"
  }
}
