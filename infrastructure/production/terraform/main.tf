# Applied GCP scaffold on project ferrum-497801. Not production-ready: internal
# smoke deploy only, single-node cluster, no public ingress, no real Slack/SMTP
# secrets, no A-07 pen test completed.
# Root module: provider configuration and shared resources.

provider "google" {
  project = var.gcp_project_id
  region  = var.gcp_region
  zone    = var.gcp_zone
}

# Kubernetes provider is configured after GKE cluster creation.
# provider "kubernetes" { ... }

resource "random_id" "suffix" {
  byte_length = 4
}
