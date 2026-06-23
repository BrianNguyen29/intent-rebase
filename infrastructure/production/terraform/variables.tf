# Applied GCP scaffold on project ferrum-497801. Not production-ready: internal
# smoke deploy only, single-node cluster, no public ingress, no real Slack/SMTP
# secrets, no A-07 pen test completed.
# Input variables for GCP production scaffold.

variable "gcp_project_id" {
  description = "GCP project ID. Applied to ferrum-497801; update default for reuse."
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
  default     = 2
}

variable "gke_node_machine_type" {
  description = "GKE node machine type"
  type        = string
  default     = "e2-medium"
}

variable "gcs_bucket_name" {
  description = "GCS bucket name prefix. Default is set to the actual project prefix to avoid destructive placeholder plans. Full bucket name includes the environment suffix applied in storage.tf."
  type        = string
  default     = "ire-prod-ferrum-497801"
}

variable "db_password" {
  description = "Cloud SQL application user password. Set via TF_VAR_db_password at apply time; never commit a default or tfvars file."
  type        = string
  sensitive   = true
}

# Forensic Bucket Lock guard — intentionally defaulted to false.
# A locked retention policy cannot be removed without destroying the bucket,
# which makes cost cleanup impossible for the retention period.
# Set to true ONLY after explicit owner approval and documented legal-hold
# requirements. Not a substitute for S3 Object Lock compliance mode.
variable "forensic_bucket_lock_enabled" {
  description = "Enable GCS Bucket Lock (is_locked = true) on the forensic-evidence bucket. WARNING: irreversible without bucket destruction. Default false for safety."
  type        = bool
  default     = false
}