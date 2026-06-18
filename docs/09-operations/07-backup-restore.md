# 07 — Backup & Restore Procedures

**Status:** `LOCAL VALIDATED — Templates + Docker-Compose Non-Destructive Restore Verified`
**Phase:** Phase 3 — Ops Evidence Track
**Owner:** Backend Lead (solo practitioner)
**Last Updated:** 2026-06-18

---

## Purpose

This document provides **procedure templates** for backup and restore operations targeting **RPO = 1 hour** and **RTO = 30 minutes**. These are documented procedures for future execution — they have NOT been executed against production infrastructure.

> **⚠️ Evidence Strength Disclaimer**
>
> These are **procedure templates and playbooks**, not executed production backups. Do not represent these procedures as having been run against production. Real backup/restore validation requires production infrastructure and external SRE sign-off.

---

## Local Validation Evidence (Phase 3 I6)

> **Scope:** Non-destructive `pg_dump` / `pg_restore` validation against the local docker-compose PostgreSQL instance. This is **not** a production PITR/basebackup validation, nor a destructive incident restore.

A local backup/restore round-trip was executed on `intent_rebase_phase1_fix` to a separate restore database (`intent_rebase_i6_restore`) to verify that:
1. A `pg_dump` produces a restorable archive.
2. `pg_restore` recreates the schema and data faithfully.
3. Migrations and application tests pass against the restored database.

### Execution Log

| Step | Command | Result |
|------|---------|--------|
| 1 | `docker compose -f infrastructure/local/docker-compose.yml up -d postgres` | Postgres healthy after startup |
| 2 | `docker exec intent-rebase-postgres pg_dump -U intent_rebase -Fc -d intent_rebase_phase1_fix -f /tmp/i6_restore_test.dump` | Passed |
| 3 | `docker exec intent-rebase-postgres ls -l /tmp/i6_restore_test.dump` | `156498` bytes |
| 4 | `docker exec intent-rebase-postgres dropdb -U intent_rebase --if-exists intent_rebase_i6_restore` | Skipped (absent) |
| 5 | `docker exec intent-rebase-postgres createdb -U intent_rebase intent_rebase_i6_restore` | Passed |
| 6 | `docker exec intent-rebase-postgres pg_restore -U intent_rebase -d intent_rebase_i6_restore /tmp/i6_restore_test.dump` | Passed |
| 7 | `docker exec intent-rebase-postgres psql -U intent_rebase -d intent_rebase_i6_restore -c "SELECT COUNT(*) AS migrations FROM _sqlx_migrations"` | `21` rows |
| 8 | `DATABASE_URL=postgres://intent_rebase:intent_rebase_dev@localhost:5432/intent_rebase_i6_restore cargo test -p intent-service --test migration_integration -- --ignored` | **1/1 passed** |
| 9 | `DATABASE_URL=postgres://intent_rebase:intent_rebase_dev@localhost:5432/intent_rebase_i6_restore cargo test -p intent-api --test webhook_integration -- --ignored` | **1/1 passed** |

### Interpretation

- The restored database contains all 21 `_sqlx_migrations` rows, indicating schema fidelity.
- The migration integration test passes against the restore target, confirming the restored schema is functional for application tests.
- The webhook integration test passes against the restore target, confirming data and outbox/subscription schema are intact after restore.

> **⚠️ Caveats**
>
> - This is `pg_dump`/`pg_restore` into a **separate** database, not `pg_basebackup` + WAL PITR.
> - No production infrastructure was involved; no RPO/RTO targets were measured.
> - No destructive overwrite of the source database occurred.
> - Production backup/restore validation (basebackup, WAL archiving, PITR, offsite replication) remains deferred to Phase 4+ with external SRE sign-off.

---

## Target Recovery Objectives

| Objective | Target | Definition |
|-----------|--------|------------|
| **RPO** | ≤ 1 hour | Maximum acceptable data loss window |
| **RTO** | ≤ 30 minutes | Maximum acceptable downtime for restore |

These targets inform backup frequency and restore procedure priority, but do not constitute a production-ready commitment until external SRE review confirms feasibility.

---

## Component Inventory

| Component | Data Type | Backup Method | Restore Method |
|-----------|-----------|---------------|----------------|
| **PostgreSQL** | Intent metadata, audit events, policy snapshots, approval records | pg_basebackup + continuous WAL archiving | Point-in-time recovery (PITR) from WAL |
| **NATS/JetStream** | Event stream, consumer state, stream metadata | JetStream backup (nats-server backup) | JetStream restore |
| **MinIO (S3)** | Policy snapshot blobs, artifact storage | MinIO bucket replication / `mc mirror` | Restore from replicated bucket |
| **Application State** | Intent-api in-memory state | N/A — stateless service; replay from PostgreSQL + NATS | Restart service; Kafka consumer replay |

---

## Cloud SQL PITR Restore Procedure (GCP)

> **⚠️ NOT YET EXECUTED — Procedure Only**
>
> This section documents the intended Point-in-Time Recovery (PITR) procedure for the provisioned Cloud SQL Postgres instance (`production-template-postgres-ed2c5bdd`). The procedure has **not been executed** against the live instance. RPO/RTO targets are documented but not measured. Execute this only after scheduling a maintenance window and confirming with the SRE owner.

### Prerequisites

- GCP project `ferrum-497801` with Cloud SQL Admin API enabled.
- `gcloud` CLI authenticated with sufficient permissions (`roles/cloudsql.admin` or `roles/editor`).
- A target recovery time (RFC 3339) within the PITR window (Cloud SQL PITR is enabled via `point_in_time_recovery_enabled = true` in Terraform).
- Sufficient quota for a second Cloud SQL instance if restoring to a new instance (recommended to avoid overwriting the source).

### Restore to a New Instance (Recommended)

```bash
# 1. List available restore points (last 7 days for Cloud SQL Enterprise)
gcloud sql backups list --instance=production-template-postgres-ed2c5bdd --project=ferrum-497801

# 2. Restore to a new instance at a specific point in time
#    Replace RESTORE_TIME with an RFC 3339 timestamp within the PITR window.
RESTORE_TIME="2026-06-18T12:00:00.000Z"
NEW_INSTANCE_NAME="production-template-postgres-restore-$(date +%s)"

gcloud sql instances clone production-template-postgres-ed2c5bdd \
  --project=ferrum-497801 \
  --destination-instance-name="${NEW_INSTANCE_NAME}" \
  --point-in-time="${RESTORE_TIME}"

# 3. Verify the new instance is healthy
gcloud sql instances describe "${NEW_INSTANCE_NAME}" --project=ferrum-497801

# 4. Connect and verify schema/data fidelity
#    Update the connection string to use the new instance's private IP.
#    DATABASE_URL="postgres://intent_rebase_app:REAL_PASSWORD@NEW_PRIVATE_IP:5432/intent_rebase"
#    psql "${DATABASE_URL}" -c "SELECT COUNT(*) FROM _sqlx_migrations;"
#    psql "${DATABASE_URL}" -c "SELECT COUNT(*) FROM intents;"

# 5. (Optional) Run application smoke tests against the restored instance
#    DATABASE_URL=... cargo test -p intent-service --test migration_integration -- --ignored
#    DATABASE_URL=... cargo test -p intent-api --test webhook_integration -- --ignored

# 6. If validation passes, coordinate cutover with SRE
#    - Update the application DATABASE_URL to point to the new instance.
#    - Or, delete the old instance and rename the new one (downtime required).

# 7. Clean up the temporary restore instance if not needed for ongoing testing
# gcloud sql instances delete "${NEW_INSTANCE_NAME}" --project=ferrum-497801 --quiet
```

### Restore in Place (Destructive — Not Recommended Without Approval)

```bash
# ⚠️ WARNING: This overwrites the existing instance. Do not run without explicit SRE approval.
# gcloud sql instances restore production-template-postgres-ed2c5bdd \
#   --project=ferrum-497801 \
#   --backup-id=BACKUP_ID
```

### Validation Checklist (To Be Completed When Executed)

| Step | Check | Expected Result | Actual Result | Pass/Fail |
|------|-------|-----------------|---------------|-----------|
| 1 | Clone command completes | New instance enters `RUNNABLE` | | |
| 2 | Schema migration count | `SELECT COUNT(*) FROM _sqlx_migrations` = 21 | | |
| 3 | Intent table row count | Matches pre-restore approximate count | | |
| 4 | Application migration integration test | `cargo test -p intent-service --test migration_integration` passes | | |
| 5 | Application webhook integration test | `cargo test -p intent-api --test webhook_integration` passes | | |
| 6 | RPO measurement | Data loss ≤ 1 hour from target restore time | | |
| 7 | RTO measurement | Clone + validation completed ≤ 30 minutes | | |

### Forbidden Claims

| Forbidden Claim | Allowed Replacement |
|----------------|-------------------|
| `PITR restore tested on production` | `PITR procedure documented; execution scheduled for next maintenance window` |
| `RPO/RTO SLA validated` | `Target RPO=1h/RTO=30m documented; validation pending execution against Cloud SQL instance` |
| `Cloud SQL backups are immutable` | `Cloud SQL automated backups enabled; immutability not equivalent to S3 Object Lock` |

---

## PostgreSQL Backup & Restore

### Backup Procedure

> **Template — Execute before production deployment**

```bash
#!/bin/bash
# postgres-backup.sh — PostgreSQL Backup Procedure Template
# Frequency: Every 1 hour (RPO = 1h)
# Target RTO: 30 minutes for PostgreSQL layer

set -euo pipefail

BACKUP_DIR="${BACKUP_DIR:-/var/backups/postgres}"
WAL_DIR="${WAL_DIR:-/var/backups/postgres/wal}"
RETENTION_DAYS="${RETENTION_DAYS:-168}"  # 7 days
PGHOST="${PGHOST:-localhost}"
PGPORT="${PGPORT:-5432}"
PGUSER="${PGUSER:-intent_rebase}"

TIMESTAMP=$(date +%Y%m%d_%H%M%S)
BACKUP_NAME="pg_basebackup_${TIMESTAMP}"

# Ensure backup directories exist
mkdir -p "${BACKUP_DIR}" "${WAL_DIR}"

# 1. pg_basebackup — full base backup
echo "[$(date -Iseconds)] Starting pg_basebackup..."
pg_basebackup \
  -h "${PGHOST}" \
  -p "${PGPORT}" \
  -U "${PGUSER}" \
  -D "${BACKUP_DIR}/${BACKUP_NAME}" \
  -Ft \
  -z \
  -P \
  -X stream \
  --checkpoint=fast

# 2. Compress and tag
cd "${BACKUP_DIR}"
tar -czf "${BACKUP_NAME}.tar.gz" "${BACKUP_NAME}"
rm -rf "${BACKUP_NAME}"

# 3. Upload to S3/MinIO (offsite)
# mc mirror "${BACKUP_DIR}/${BACKUP_NAME}.tar.gz" minio/ire-postgres-backups/

# 4. Prune old backups
find "${BACKUP_DIR}" -name "pg_basebackup_*.tar.gz" -mtime +${RETENTION_DAYS} -delete
echo "[$(date -Iseconds)] Backup complete: ${BACKUP_NAME}.tar.gz"
```

### Restore Procedure

> **Template — Execute only during incident recovery**

```bash
#!/bin/bash
# postgres-restore.sh — PostgreSQL Restore Procedure Template
# Target RTO: 30 minutes
# WARNING: This will overwrite the current database — execute only during controlled restore

set -euo pipefail

BACKUP_DIR="${BACKUP_DIR:-/var/backups/postgres}"
TARGET_BACKUP="${TARGET_BACKUP:-}"  # Set to specific backup name
PGHOST="${PGHOST:-localhost}"
PGPORT="${PGPORT:-5432}"
PGDATA="${PGDATA:-/var/lib/postgresql/data}"

echo "[$(date -Iseconds)] WARNING: Starting PostgreSQL restore..."
echo "[$(date -Iseconds)] Target backup: ${TARGET_BACKUP}"
echo "[$(date -Iseconds)] Target host: ${PGHOST}:${PGPORT}"

# 1. Stop intent-api to prevent writes
# systemctl stop intent-api

# 2. Stop PostgreSQL
# systemctl stop postgresql

# 3. Backup current data directory (if any)
if [ -d "${PGDATA}" ]; then
  mv "${PGDATA}" "${PGDATA}.pre_restore_$(date +%Y%m%d_%H%M%S)"
fi

# 4. Extract backup
mkdir -p "${PGDATA}"
cd "${BACKUP_DIR}"
tar -xzf "${TARGET_BACKUP}.tar.gz" -C "${PGDATA}"

# 5. Set permissions
chown -R postgres:postgres "${PGDATA}"
chmod 700 "${PGDATA}"

# 6. Start PostgreSQL and verify
# systemctl start postgresql

# 7. Verify connectivity
# pg_isready -h "${PGHOST}" -p "${PGPORT}"

echo "[$(date -Iseconds)] PostgreSQL restore complete. Verify data integrity before restarting intent-api."
```

### Point-in-Time Recovery (PITR)

> **Template — For RPO < 1 hour scenarios**

PostgreSQL PITR allows recovery to any point within the WAL retention window:

```bash
#!/bin/bash
# postgres-pitr-restore.sh — Point-in-Time Recovery Template
# Use when: Data must be recovered to a specific timestamp (e.g., before a bad write)
# RTO: Depends on WAL volume — estimate based on WAL archive size

set -euo pipefail

RECOVERY_TARGET_TIME="${RECOVERY_TARGET_TIME:-}"  # ISO8601 timestamp, e.g., "2026-04-29 10:00:00 UTC"
BACKUP_DIR="${BACKUP_DIR:-/var/backups/postgres}"
PGDATA="${PGDATA:-/var/lib/postgresql/data}"

# Create recovery signal file
touch "${PGDATA}/recovery.signal"

# Write recovery.conf (PostgreSQL < 12) or postgresql.conf (PostgreSQL >= 12)
# For PostgreSQL >= 12, set in postgresql.conf:
cat >> "${PGDATA}/postgresql.conf" <<EOF
restore_command = 'gunzip -c ${WAL_DIR}/%f > %p'
recovery_target_time = '${RECOVERY_TARGET_TIME}'
recovery_target_action = 'promote'
EOF

echo "[$(date -Iseconds)] PITR configured. Target: ${RECOVERY_TARGET_TIME}"
echo "[$(date -Iseconds)] Start PostgreSQL to begin recovery."
```

---

## NATS/JetStream Backup & Restore

### Backup Procedure

> **Template — Execute periodically (RPO = 1h)**

```bash
#!/bin/bash
# nats-jetstream-backup.sh — NATS/JetStream Backup Procedure Template
# Frequency: Every 1 hour (aligned with PostgreSQL RPO)
# Target RTO: ~10 minutes for NATS layer

set -euo pipefail

NATS_HOST="${NATS_HOST:-localhost}"
NATS_PORT="${NATS_PORT:-4222}"
BACKUP_DIR="${BACKUP_DIR:-/var/backups/nats}"
RETENTION_DAYS="${RETENTION_DAYS:-72}"  # 3 days

TIMESTAMP=$(date +%Y%m%d_%H%M%S)
BACKUP_NAME="nats_jetstream_backup_${TIMESTAMP}"

mkdir -p "${BACKUP_DIR}"

# Use nats-server backup command (requires nats-server admin CLI)
# Note: JetStream backup captures stream state, consumer state, and message data
echo "[$(date -Iseconds)] Starting JetStream backup to ${BACKUP_DIR}/${BACKUP_NAME}..."
# nats-server backup "${BACKUP_DIR}/${BACKUP_NAME}" --exclude ">"
# For current implementation, use stream dump as proxy:

# List all streams
STREAMS=$(nats stream ls --server "nats://${NATS_HOST}:${NATS_PORT}" 2>/dev/null || echo "")

# For each stream, export messages (bounded by --count limit)
for STREAM in ${STREAMS}; do
  echo "[$(date -Iseconds)] Backing up stream: ${STREAM}"
  # nats stream export "${STREAM}" "${BACKUP_DIR}/${STREAM}_${TIMESTAMP}.json" --count 10000
done

# Compress
cd "${BACKUP_DIR}"
tar -czf "${BACKUP_NAME}.tar.gz" *.json 2>/dev/null || true
rm -f *.json 2>/dev/null || true

# Prune old backups
find "${BACKUP_DIR}" -name "nats_jetstream_backup_*.tar.gz" -mtime +${RETENTION_DAYS} -delete

echo "[$(date -Iseconds)] JetStream backup complete: ${BACKUP_NAME}.tar.gz"
```

### Restore Procedure

> **Template — Execute only during incident recovery**

```bash
#!/bin/bash
# nats-jetstream-restore.sh — NATS/JetStream Restore Procedure Template
# WARNING: This will overwrite current stream state

set -euo pipefail

BACKUP_DIR="${BACKUP_DIR:-/var/backups/nats}"
TARGET_BACKUP="${TARGET_BACKUP:-}"  # Set to specific backup name
NATS_HOST="${NATS_HOST:-localhost}"
NATS_PORT="${NATS_PORT:-4222}"

echo "[$(date -Iseconds)] WARNING: Starting JetStream restore..."
echo "[$(date -Iseconds)] Target backup: ${TARGET_BACKUP}"

# 1. Stop NATS server
# systemctl stop nats-server

# 2. Extract backup
cd "${BACKUP_DIR}"
tar -xzf "${TARGET_BACKUP}.tar.gz"

# 3. Restore via nats-server restore
# nats-server restore "${BACKUP_DIR}/${TARGET_BACKUP}"

# 4. Restart NATS server
# systemctl start nats-server

# 5. Verify streams
# nats stream ls --server "nats://${NATS_HOST}:${NATS_PORT}"

echo "[$(date -Iseconds)] JetStream restore complete. Verify stream contents."
```

---

## MinIO/S3 Backup & Restore

### Backup Procedure

> **Template — Execute periodically (RPO = 1h)**

```bash
#!/bin/bash
# minio-backup.sh — MinIO/S3 Backup Procedure Template
# Frequency: Every 1 hour (aligned with PostgreSQL RPO)
# Target RTO: ~10 minutes for MinIO layer (bucket-level)
# Note: Object Lock NOT enabled in Phase 3 — see docs/14-governance/05b-s3-option-b-decision.md

set -euo pipefail

MINIO_ENDPOINT="${MINIO_ENDPOINT:-localhost:9000}"
MINIO_ACCESS_KEY="${MINIO_ACCESS_KEY:-minioadmin}"
MINIO_SECRET_KEY="${MINIO_SECRET_KEY:-minioadmin}"
MINIO_BUCKETS="${MINIO_BUCKETS:-ire-policy-snapshots ire-artifacts}"
BACKUP_DIR="${BACKUP_DIR:-/var/backups/minio}"
RETENTION_DAYS="${RETENTION_DAYS:-72}"

TIMESTAMP=$(date +%Y%m%d_%H%M%S)
mkdir -p "${BACKUP_DIR}"

# Set mc alias (if not already set)
# mc alias set local "http://${MINIO_ENDPOINT}" "${MINIO_ACCESS_KEY}" "${MINIO_SECRET_KEY}"

for BUCKET in ${MINIO_BUCKETS}; do
  echo "[$(date -Iseconds)] Backing up bucket: ${BUCKET}"
  # mc mirror --preserve bucket local/"${BUCKET}_${TIMESTAMP}"/
  # For Phase 3: mc mirror local/"${BUCKET}" "${BACKUP_DIR}/${BUCKET}_${TIMESTAMP}/"
done

# Create archive
cd "${BACKUP_DIR}"
tar -czf "minio_backup_${TIMESTAMP}.tar.gz" */

# Prune old backups
find "${BACKUP_DIR}" -name "minio_backup_*.tar.gz" -mtime +${RETENTION_DAYS} -delete

echo "[$(date -Iseconds)] MinIO backup complete: minio_backup_${TIMESTAMP}.tar.gz"
```

### Restore Procedure

> **Template — Execute only during incident recovery**

```bash
#!/bin/bash
# minio-restore.sh — MinIO/S3 Restore Procedure Template
# WARNING: This will overwrite current bucket contents

set -euo pipefail

BACKUP_DIR="${BACKUP_DIR:-/var/backups/minio}"
TARGET_BACKUP_DIR="${TARGET_BACKUP_DIR:-}"  # Set to specific backup subdirectory
MINIO_ENDPOINT="${MINIO_ENDPOINT:-localhost:9000}"
MINIO_ACCESS_KEY="${MINIO_ACCESS_KEY:-minioadmin}"
MINIO_SECRET_KEY="${MINIO_SECRET_KEY:-minioadmin}"

echo "[$(date -Iseconds)] WARNING: Starting MinIO restore..."
echo "[$(date -Iseconds)] Target backup dir: ${TARGET_BACKUP_DIR}"

# mc alias set local "http://${MINIO_ENDPOINT}" "${MINIO_ACCESS_KEY}" "${MINIO_SECRET_KEY}"

# Restore each bucket
# for DIR in "${BACKUP_DIR}/${TARGET_BACKUP_DIR}"/*/; do
#   BUCKET_NAME=$(basename "${DIR}")
#   echo "[$(date -Iseconds)] Restoring bucket: ${BUCKET_NAME}"
#   mc mirror --preserve "${DIR}" local/"${BUCKET_NAME}"
# done

echo "[$(date -Iseconds)] MinIO restore complete. Verify bucket contents."
```

---

## Composite Restore Sequence

> **Template — Execute during full system restore incident**

When restoring from a multi-component failure:

```bash
#!/bin/bash
# full-system-restore.sh — Composite Restore Procedure Template
# Target RTO: 30 minutes total
# Order: PostgreSQL (source of truth) -> MinIO (blobs) -> NATS (event replay)

set -euo pipefail

# STOPPING PHASE (T-0)
echo "[$(date -Iseconds)] STOPPING: Halting write traffic..."
# kubectl scale deployment intent-api --replicas=0
# or: systemctl stop intent-api

# STEP 1: PostgreSQL Restore (~10 minutes)
echo "[$(date -Iseconds)] STEP 1: Restoring PostgreSQL..."
# ./postgres-restore.sh --target "${PG_BACKUP}"
# Verify: pg_isready && psql -c "SELECT count(*) FROM audit_events;"

# STEP 2: MinIO Restore (~5 minutes)
echo "[$(date -Iseconds)] STEP 2: Restoring MinIO..."
# ./minio-restore.sh --source "${MINIO_BACKUP_DIR}"

# STEP 3: Verify MinIO Objects
echo "[$(date -Iseconds)] STEP 3: Verifying MinIO object integrity..."
# mc stat local/ire-policy-snapshots/*/v*/snapshot.json | head -20

# STEP 4: NATS Restore (if needed, ~10 minutes)
echo "[$(date -Iseconds)] STEP 4: Restoring NATS/JetStream (if needed)..."
# ./nats-jetstream-restore.sh --target "${NATS_BACKUP}"
# Note: If NATS backup is unavailable, replay from PostgreSQL audit events

# STEP 5: Restart Application
echo "[$(date -Iseconds)] STEP 5: Restarting intent-api..."
# systemctl start intent-api
# or: kubectl scale deployment intent-api --replicas=3

# STEP 6: Verify Application Health
echo "[$(date -Iseconds)] STEP 6: Verifying application health..."
# curl -s http://localhost:8080/health | jq .
# Verify audit event flow: POST a test intent, check audit_events table

echo "[$(date -Iseconds)] RESTORE COMPLETE. Total RTO: $(($(date +%s) - START_TIME)) seconds"
```

---

## Backup Verification Checklist

> **Template — Execute after each backup**

| Check | Command | Expected |
|-------|---------|----------|
| PostgreSQL backup file exists | `ls -la ${BACKUP_DIR}/pg_basebackup_*.tar.gz` | File size > 0 |
| PostgreSQL backup is valid | `pg_basebackup --verify --checkpoint=fast` | Exit code 0 |
| MinIO bucket reachable | `mc ls local/ire-policy-snapshots/` | List of objects |
| NATS streams exist | `nats stream ls` | `audit_events` stream present |
| Backup timestamp within RPO | `find ${BACKUP_DIR} -name "*.tar.gz" -mtime -1 \| wc -l` | ≥ 1 |

---

## Deferred Items (Phase 4+)

The following are NOT in Phase 3 scope:

| Item | Reason Deferred | Phase |
|------|----------------|-------|
| Object Lock (GOVERNANCE/COMPLIANCE) | Phase 4+ scope | Phase 4 |
| Cross-region replication | Requires production multi-region | Phase 4+ |
| Continuous WAL shipping | Requires external monitoring | Phase 4+ |
| Automated restore testing | Requires staging environment | Phase 4+ |
| Backup encryption at rest | Requires KMS integration | Phase 4+ |

---

## Relationship to Other Documents

| Document | Relationship |
|----------|--------------|
| `docs/14-governance/05-immutable-retention-tamper-resistance.md` | Backup complements immutability — backups enable recovery; Object Lock protects data |
| `docs/14-governance/05-s3-snapshot-blob-spec.md` | MinIO stores policy snapshot blobs; backup protects blob store |
| `docs/09-operations/08-secrets-inventory.md` | Backup procedures must include secrets rotation schedule |
| `infrastructure/staging/docker-compose.yml` | Staging scaffold for backup/restore validation |

---

## Forbidden Claims

| Forbidden Claim | Allowed Replacement |
|----------------|-------------------|
| `Backup/restore tested in production` | `Procedures documented; execution requires production infrastructure` |
| `RPO/RTO SLA met` | `Target RPO=1h/RTO=30m documented; not verified against production` |
| `Backups are immutable` | `Phase 3: Backups are not Object-Lock protected (Phase 4+)` |

---

## Update Log

| Date | Updated By | Changes |
|------|------------|---------|
| April 2026 | (fixer) | Initial creation — PostgreSQL/NATS/MinIO backup/restore procedure templates; composite restore sequence; verification checklist; deferred items list |
