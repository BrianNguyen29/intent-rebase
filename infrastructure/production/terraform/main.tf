# TEMPLATE ONLY — not applied; do not claim production-ready.
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
