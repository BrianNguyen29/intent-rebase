# Applied GCP scaffold on project ferrum-497801. Not production-ready: internal
# smoke deploy only, single-node cluster, no public ingress, no real Slack/SMTP
# secrets, no A-07 pen test completed.
# Remote Terraform state backend (GCS).
# NOTE: State may contain sensitive values; bucket access must be tightly controlled.

terraform {
  backend "gcs" {
    bucket = "ire-tfstate-ferrum-497801"
    prefix = "intent-rebase/production-template"
  }
}
