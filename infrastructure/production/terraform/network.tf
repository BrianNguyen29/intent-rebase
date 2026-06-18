# Applied GCP scaffold on project ferrum-497801. Not production-ready: internal
# smoke deploy only, single-node cluster, no public ingress, no real Slack/SMTP
# secrets, no A-07 pen test completed.
# VPC and subnets for private Cloud SQL and GKE.

resource "google_compute_network" "vpc" {
  name                    = "${var.environment}-vpc"
  auto_create_subnetworks = false
}

resource "google_compute_subnetwork" "subnet" {
  name          = "${var.environment}-subnet"
  ip_cidr_range = "10.0.0.0/24"
  network       = google_compute_network.vpc.id
  region        = var.gcp_region

  private_ip_google_access = true
}

resource "google_compute_global_address" "private_ip_alloc" {
  name          = "${var.environment}-private-ip-alloc"
  purpose       = "VPC_PEERING"
  address_type  = "INTERNAL"
  prefix_length = 16
  network       = google_compute_network.vpc.id
}

resource "google_service_networking_connection" "private_vpc_connection" {
  network                 = google_compute_network.vpc.id
  service                 = "servicenetworking.googleapis.com"
  reserved_peering_ranges = [google_compute_global_address.private_ip_alloc.name]
}
