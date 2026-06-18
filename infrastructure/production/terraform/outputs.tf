# Applied GCP scaffold on project ferrum-497801. Not production-ready: internal
# smoke deploy only, single-node cluster, no public ingress, no real Slack/SMTP
# secrets, no A-07 pen test completed.

output "vpc_id" {
  description = "VPC network ID"
  value       = google_compute_network.vpc.id
}

output "postgres_instance_name" {
  description = "Cloud SQL instance name"
  value       = google_sql_database_instance.postgres.name
}

output "postgres_private_ip" {
  description = "Cloud SQL private IP (if available)"
  value       = google_sql_database_instance.postgres.ip_address
}

output "gke_cluster_name" {
  description = "GKE cluster name"
  value       = google_container_cluster.primary.name
}

output "gke_cluster_endpoint" {
  description = "GKE cluster endpoint"
  value       = google_container_cluster.primary.endpoint
}

output "gcs_bucket_name" {
  description = "GCS bucket name"
  value       = google_storage_bucket.artifacts.name
}
