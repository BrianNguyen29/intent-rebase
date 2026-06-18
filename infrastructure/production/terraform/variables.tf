# TEMPLATE ONLY — not applied; do not claim production-ready.
# Input variables for GCP production scaffold.

variable "gcp_project_id" {
  description = "GCP project ID. TEMPLATE ONLY — replace with real project."
  type        = string
  default     = "CHANGE_ME_GCP_PROJECT"
}

variable "gcp_region" {
  description = "GCP region"
  type        = string
  default     = "us-central1"
}

variable "gcp_zone" {
  description = "GCP zone"
  type        = string
  default     = "us-central1-a"
}

variable "environment" {
  description = "Environment name"
  type        = string
  default     = "production-template"
}

variable "postgres_tier" {
  description = "Cloud SQL machine tier"
  type        = string
  default     = "db-f1-micro"
}

variable "gke_node_count" {
  description = "Initial GKE node count"
  type        = number
  default     = 1
}

variable "gke_node_machine_type" {
  description = "GKE node machine type"
  type        = string
  default     = "e2-medium"
}

variable "gcs_bucket_name" {
  description = "GCS bucket name prefix"
  type        = string
  default     = "CHANGE_ME_GCS_BUCKET"
}

variable "db_password" {
  description = "Cloud SQL application user password. Set via TF_VAR_db_password at apply time; never commit a default or tfvars file."
  type        = string
  sensitive   = true
}
