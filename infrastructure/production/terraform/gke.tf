# TEMPLATE ONLY — not applied; do not claim production-ready.
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

  # TEMPLATE ONLY — enable workload identity and other hardening before production

  deletion_protection = false # Scaffold/test apply only; re-enable before production
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
