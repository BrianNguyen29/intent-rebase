# Applied GCP scaffold on project ferrum-497801. Not production-ready: internal
# smoke deploy only, single-node cluster, no public ingress, no real Slack/SMTP
# secrets, no A-07 pen test completed.
# GCS bucket with retention policy and uniform access.
# NOTE: GCP retention policy is not S3 Object Lock. Object Lock compliance
# mode is not available on GCS. Use this as a placeholder for immutable
# storage planning; actual Object Lock may require S3-compatible storage
# or multi-cloud design (see A-13).

resource "google_storage_bucket" "artifacts" {
  name          = "${var.gcs_bucket_name}-${var.environment}-${random_id.suffix.hex}"
  location      = var.gcp_region
  force_destroy = false

  uniform_bucket_level_access = true
  public_access_prevention    = "enforced"

  retention_policy {
    retention_period = 2592000 # 30 days in seconds
    is_locked        = false
  }

  versioning {
    enabled = true
  }
}

resource "google_storage_bucket" "forensic-evidence" {
  # NOTE: This resource name was too long (>63 chars) for GCS:
  # "${var.gcs_bucket_name}-forensic-evidence-${var.environment}-${random_id.suffix.hex}"
  # The bucket was created via gcloud instead because the remote Terraform state
  # (GCS backend) is inaccessible from the current environment.
  # name = "forensic-evidence-ferrum-497801-ed2c5bdd" # Created via gcloud 2026-06-22
  name          = "forensic-evidence-${var.gcp_project_id}-${random_id.suffix.hex}"
  location      = var.gcp_region
  force_destroy = false

  uniform_bucket_level_access = true
  public_access_prevention    = "enforced"

  retention_policy {
    retention_period = 2592000 # 30 days in seconds
    is_locked        = false   # Bucket Lock intentionally deferred to avoid irreversible cost lock-in
  }

  versioning {
    enabled = true
  }

  lifecycle_rule {
    action {
      type = "Delete"
    }
    condition {
      age = 365
    }
  }
}

# NOTE: Bucket Lock (is_locked = true) is intentionally NOT enabled.
# A locked retention policy cannot be removed without destroying the bucket,
# which makes cost cleanup impossible for the retention period. Lock only
# after explicit approval and legal-hold requirements are documented.
