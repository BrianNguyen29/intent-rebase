# Applied GCP scaffold on project ferrum-497801. Not production-ready: internal
# smoke deploy only, single-node cluster, no public ingress, no real Slack/SMTP
# secrets, no A-07 pen test completed.
# Terraform version and provider constraints for GCP production scaffold.

terraform {
  required_version = ">= 1.5.0"

  # NOTE: GCS backend is configured in backend.tf; state migrated to
  # bucket ire-tfstate-ferrum-497801. This block is a legacy placeholder.

  required_providers {
    google = {
      source  = "hashicorp/google"
      version = "~> 6.0"
    }
    kubernetes = {
      source  = "hashicorp/kubernetes"
      version = "~> 3.0"
    }
    random = {
      source  = "hashicorp/random"
      version = "~> 3.0"
    }
  }
}
