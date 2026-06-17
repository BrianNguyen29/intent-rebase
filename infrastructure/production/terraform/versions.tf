# TEMPLATE ONLY — not applied; do not claim production-ready.
# Terraform version and provider constraints for GCP production scaffold.

terraform {
  required_version = ">= 1.5.0"

  # NOTE: Add a GCS backend before any team apply:
  # backend "gcs" { bucket = "CHANGE_ME_TFSTATE_BUCKET" prefix = "terraform/state" }

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
