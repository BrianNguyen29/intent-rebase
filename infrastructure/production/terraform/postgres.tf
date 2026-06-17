# TEMPLATE ONLY — not applied; do not claim production-ready.
# Cloud SQL PostgreSQL with backups, PITR, private IP, and deletion protection.

resource "google_sql_database_instance" "postgres" {
  name             = "${var.environment}-postgres-${random_id.suffix.hex}"
  database_version = "POSTGRES_16"
  region           = var.gcp_region

  settings {
    tier = var.postgres_tier

    ip_configuration {
      ipv4_enabled    = false
      private_network = google_compute_network.vpc.id
    }

    backup_configuration {
      enabled                        = true
      start_time                     = "03:00"
      point_in_time_recovery_enabled = true
    }
  }

  deletion_protection = true
}

resource "google_sql_database" "app" {
  name     = "intent_rebase"
  instance = google_sql_database_instance.postgres.name
}

resource "google_sql_user" "app" {
  name     = "intent_rebase_app"
  instance = google_sql_database_instance.postgres.name
  password = "CHANGE_ME_DB_PASSWORD" # Must be replaced with a real secret before apply
}
