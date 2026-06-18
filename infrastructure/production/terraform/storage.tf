# TEMPLATE ONLY — not applied; do not claim production-ready.
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
