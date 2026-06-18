# Applied GCP scaffold on project ferrum-497801. Not production-ready: internal
# smoke deploy only, single-node cluster, no public ingress, no real Slack/SMTP
# secrets, no A-07 pen test completed.
# GKE cluster and node pool.

resource "google_service_account" "gke" {
  account_id   = "${var.environment}-gke-sa"
  display_name = "GKE Service Account"
}

resource "google_container_cluster" "primary" {
  name     = "${var.environment}-gke"
  location = var.gcp_zone

  remove_default_node_pool = true
  initial_node_count       = 1

  network    = google_compute_network.vpc.name
  subnetwork = google_compute_subnetwork.subnet.name

  # Enable private nodes / private endpoint in production before apply
  # private_cluster_config { ... }

  # Hardening still required before production: enable workload identity, private
  # nodes, and private endpoint. Deletion protection enabled for Phase 1 stable
  # internal infrastructure.

  deletion_protection = true
}

resource "google_container_node_pool" "primary" {
  name       = "${var.environment}-node-pool"
  location   = var.gcp_zone
  cluster    = google_container_cluster.primary.name
  node_count = var.gke_node_count

  node_config {
    machine_type = var.gke_node_machine_type
    disk_size_gb = 30

    service_account = google_service_account.gke.email
    oauth_scopes = [
      "https://www.googleapis.com/auth/cloud-platform",
    ]
  }
}
