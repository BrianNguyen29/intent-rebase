# 07 — Backup & Restore Procedures

**Status:** `LOCAL VALIDATED — Templates + Docker-Compose Non-Destructive Restore Verified`
**Phase:** Phase 3 — Ops Evidence Track
**Owner:** Backend Lead (solo practitioner)
**Last Updated:** 2026-06-18

---

## Purpose

This document provides **procedure templates** for backup and restore operations targeting **RPO = 1 hour** and **RTO = 30 minutes**. **Cloud SQL PITR clone restore was validated against a separate Cloud SQL clone on 2026-06-18** (see §Execution Evidence). PostgreSQL basebackup/WAL archiving, MinIO/S3, and NATS/JetStream backup/restore procedures remain documented procedures for future execution — they have NOT been executed against production infrastructure.

> **⚠️ Evidence Strength Disclaimer**
>
> **Cloud SQL PITR clone restore was validated against a separate Cloud SQL clone on 2026-06-18** (see §Execution Evidence). PostgreSQL basebackup/WAL archiving, MinIO/S3, and NATS/JetStream backup/restore procedures remain **procedure templates and playbooks**, not executed production backups. Do not represent these procedures as having been run against production. Real backup/restore validation for basebackup/WAL/MinIO/NATS requires production infrastructure and external SRE sign-off.

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
> - Cloud SQL PITR clone-only validated (2026-06-18); full DR program (basebackup, WAL archiving, offsite replication, live RPO/RTO measurement, scheduled drills) remains deferred to Phase 4+ with external SRE sign-off.

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

 > **⚠️ EXECUTED — CLONE-ONLY VALIDATION (2026-06-18)**
>
> This section documents the Point-in-Time Recovery (PITR) procedure for the provisioned Cloud SQL Postgres instance (`production-template-postgres-ed2c5bdd`). A PITR clone restore was **executed and validated** against a separate Cloud SQL clone on 2026-06-18. The procedure was validated; RPO/RTO targets were not measured against live production traffic. Full disaster-recovery program maturity (regular drills, offsite replication, automated restore pipelines) remains future work.
>
> **Execution evidence summary:** see §Execution Evidence below.

### Execution Evidence (2026-06-18)

| Field | Value |
|-------|-------|
| **Source instance** | `production-template-postgres-ed2c5bdd` |
| **Source state** | `RUNNABLE`, `us-central1`, `POSTGRES_16`, `db-f1-micro`, PITR enabled `True`, backups enabled `True`, deletion protection `False`, private IP `10.249.0.3` |
| **Backup used** | Automated backup `1781759356040` (`SUCCESSFUL`, `AUTOMATED`, start `2026-06-18T05:09:16.054Z`) |
| **Restore target (clone)** | `pitr-restore-test-20260618084607` |
| **Restore time** | `2026-06-18T08:41:07Z` |
| **Operation ID** | `e90e714c-bd39-4169-98b1-b5ca00000032` |
| **Operation type** | `CLONE` |
| **Operation result** | `DONE`, start `2026-06-18T08:46:36.870+00:00`, end `2026-06-18T09:04:57.913+00:00`, no error |
| **Clone post-restore state** | `RUNNABLE`, `us-central1`, `POSTGRES_16`, `db-f1-micro`, private IP `10.249.0.5`, deletion protection `False` |
| **Validation method** | Temporary Kubernetes Job `pitr-validate-20260618084607` in namespace `intent-rebase` (deleted after completion) |
| **Validation results** | `database=intent_rebase`, `public_table_count=19`, `core_tables=graph_edges,graph_nodes,intent_versions,webhook_outbox` present, `_sqlx_migrations_table=absent` |
| **Cleanup** | Clone deleted successfully; post-delete check returned `CLONE_DELETED` |

**Caveats from this execution:**
- `_sqlx_migrations` table was **absent** because the current migration Job uses raw `psql` and does not populate `sqlx` metadata. This is expected and does not indicate a PITR failure. Migration standardization (e.g., using `sqlx migrate run` or a `sqlx-cli` sidecar) is a separate work item.
- This was a **clone-only validation** against a disposable target. The source instance was never touched.
- RPO and RTO were not measured against live production traffic. The clone creation took ~18 minutes (from operation start to RUNNABLE), but this is not a production RTO measurement because no application cutover was performed.
  - Full disaster-recovery program maturity (scheduled drills, offsite replication, automated restore pipelines) remains open.

### Solo DR Drill Evidence (Partial, Non-Destructive — 2026-06-19)

> **Scope:** Non-destructive app-cutover timing validation against a Cloud SQL clone. Production was NOT scaled down or switched. This improves RTO evidence but is **not a full disaster recovery exercise**.

| Field | Value |
|-------|-------|
| **Clone name** | `dr-solo-drill-20260619134132` |
| **Source instance** | `production-template-postgres-ed2c5bdd` |
| **Start time** | `2026-06-19T13:41:32+00:00` |
| **Clone state logs** | PENDING_CREATE at 120s, 240s, 360s, 480s, 600s, 720s |
| **Clone RUNNABLE** | `DR_CLONE_READY_SECONDS=1071` (~17 minutes 51 seconds) |
| **Clone private IP** | `10.249.0.15` |
| **App cutover pod Ready** | `DR_APP_READY_SECONDS=4` |
| **Endpoint smoke** | `/health`/`/ready` fetch did **not** complete in captured output — **do not claim endpoint smoke pass for this drill** |
| **Cleanup** | Solo-drill pod(s) deleted; clone delete waited for `NO_RUNNING_OPS` then returned `CLONE_DELETED_OR_DELETING` |

**Caveats:**
- This is a **solo, non-destructive** drill. No production traffic was redirected.
- The app cutover pod became Ready quickly (`4` seconds), but endpoint responses were not captured.
- Clone provisioning time (`~17m51s`) is an infrastructure observation, not a validated production RTO.
- Full DR program (scheduled drills, live cutover, RPO/RTO measurement against production traffic, offsite replication) remains open.


### Phase 0 — Preflight (Fail-Closed)

> ⚠️ **Do not proceed to Phase 1 until all checks below pass.** If any check fails, stop and escalate to the SRE owner.

| # | Check | Command / Action | Stop Condition |
|---|-------|------------------|----------------|
| 0.1 | gcloud auth & project active | `gcloud config get-value project` returns `ferrum-497801` | ❌ STOP if not authenticated or wrong project |
| 0.2 | Source instance exists and is RUNNABLE | `gcloud sql instances describe production-template-postgres-ed2c5bdd --project=ferrum-497801` | ❌ STOP if instance is not RUNNABLE or does not exist |
| 0.3 | PITR / backup availability confirmed | `gcloud sql backups list --instance=production-template-postgres-ed2c5bdd --project=ferrum-497801` shows at least one backup within the PITR window | ❌ STOP if no backups or PITR is disabled |
| 0.4 | Beta command surface confirmed | `gcloud beta sql instances clone --help` exits 0 and lists `--point-in-time` | ❌ STOP if beta component or flag missing; update gcloud / enable component |
| 0.5 | Quota & cost acknowledged | Verify you have remaining Cloud SQL instance quota in `us-central1`. A clone creates a new billable instance for the duration of the test. | ⚠️ WARN — document cost estimate; do not proceed without budget approval |
| 0.6 | Target name is unique and not equal to source | `NEW_INSTANCE_NAME` must be a name that does NOT already exist in the project and must NOT equal `production-template-postgres-ed2c5bdd` | ❌ STOP if name collision or `NEW_INSTANCE_NAME == production-template-postgres-ed2c5bdd` |

### Phase 1 — Non-Destructive Clone to New Instance

> **The source instance `production-template-postgres-ed2c5bdd` must NEVER be the target of a restore or clone-overwrite operation.** All PITR work is performed against a **new clone**.

```bash
#!/bin/bash
set -euo pipefail

SOURCE_INSTANCE="production-template-postgres-ed2c5bdd"
PROJECT="ferrum-497801"
RESTORE_TIME="2026-06-18T12:00:00.000Z"   # RFC 3339; replace with real target
NEW_INSTANCE_NAME="production-template-postgres-restore-$(date +%s)"

# Paranoia check: source != target
if [[ "${NEW_INSTANCE_NAME}" == "${SOURCE_INSTANCE}" ]]; then
  echo "FATAL: NEW_INSTANCE_NAME equals SOURCE_INSTANCE. Aborting."
  exit 1
fi

# 1. List backups / restore points (last 7 days for Cloud SQL Enterprise)
echo "[$(date -Iseconds)] Listing available backups for ${SOURCE_INSTANCE}..."
gcloud sql backups list --instance="${SOURCE_INSTANCE}" --project="${PROJECT}"

# 2. Clone to a new instance at a specific point in time (beta command surface)
#    This creates a NEW instance; the source instance is left untouched.
echo "[$(date -Iseconds)] Cloning ${SOURCE_INSTANCE} → ${NEW_INSTANCE_NAME} at ${RESTORE_TIME}..."
gcloud beta sql instances clone "${SOURCE_INSTANCE}" \
  --project="${PROJECT}" \
  --destination-instance-name="${NEW_INSTANCE_NAME}" \
  --point-in-time="${RESTORE_TIME}"

# 3. Wait for clone to reach RUNNABLE
echo "[$(date -Iseconds)] Waiting for ${NEW_INSTANCE_NAME} to reach RUNNABLE..."
while true; do
  STATUS=$(gcloud sql instances describe "${NEW_INSTANCE_NAME}" --project="${PROJECT}" --format="value(state)")
  if [[ "${STATUS}" == "RUNNABLE" ]]; then
    echo "[$(date -Iseconds)] ${NEW_INSTANCE_NAME} is RUNNABLE."
    break
  fi
  echo "[$(date -Iseconds)] Current state: ${STATUS}. Waiting 30s..."
  sleep 30
done
```

### Phase 2 — Validation (Private-IP Connectivity)

Both the source and the clone are configured with **private IP only** (no public IP). Direct `psql` from a local workstation may fail unless you are on the authorized VPC or using a VPN/Interconnect. Use one of the following connectivity methods.

#### Option A — Connect from a GKE pod in the same VPC (preferred for validation)

```bash
# 1. Identify a running pod in the intent-rebase namespace
kubectl -n intent-rebase get pods

# 2. Exec a temporary postgres container inside the pod's network
kubectl -n intent-rebase exec -it <intent-api-pod-name> -- /bin/sh

# 3. Inside the pod, connect to the clone using its private IP
#    (obtain the clone's private IP from gcloud describe or Terraform outputs)
#    CLONE_PRIVATE_IP=$(gcloud sql instances describe "${NEW_INSTANCE_NAME}" \
#      --project="${PROJECT}" --format="value(ipAddresses[0].ipAddress)")
#    psql "postgresql://intent_rebase_app:REAL_PASSWORD@${CLONE_PRIVATE_IP}:5432/intent_rebase" \
#      -c "SELECT current_database();"
```

> **Caveat:** The clone's private IP may differ from the source. Use the clone's actual IP from `gcloud sql instances describe`.

#### Option B — Cloud SQL Auth Proxy (sidecar or local with authorized network)

```bash
# Run the proxy from a GKE pod or a Cloud Shell session with VPC connector:
# cloud-sql-proxy "${PROJECT}:${REGION}:${NEW_INSTANCE_NAME}" --private-ip &
# Then connect via localhost:5432
# psql "postgresql://intent_rebase_app:REAL_PASSWORD@localhost:5432/intent_rebase" \
#   -c "SELECT current_database();"
```

#### Validation Queries

Run these inside the connected session. `_sqlx_migrations` may be **empty** because the current migration Job uses raw `psql` and does not populate `sqlx` metadata. If `_sqlx_migrations` is empty, fall back to the table-existence checks below.

```sql
-- 2.1 Verify database connectivity and current database
SELECT current_database();

-- 2.2 Schema migration metadata (optional — may be empty)
SELECT COUNT(*) FROM _sqlx_migrations;
-- Caveat: If 0 rows, this is expected because migrations were applied via raw psql.
-- Fallback: check that application tables exist and are non-empty.

-- 2.3 Intent table row count (core application table)
SELECT COUNT(*) FROM intents;

-- 2.4 Core table existence checks (fallback if _sqlx_migrations is empty)
SELECT 'graph_nodes' AS table_name, COUNT(*) FROM graph_nodes
UNION ALL
SELECT 'graph_edges', COUNT(*) FROM graph_edges
UNION ALL
SELECT 'intent_versions', COUNT(*) FROM intent_versions
UNION ALL
SELECT 'webhook_outbox', COUNT(*) FROM webhook_outbox;
```

#### Application Smoke Tests (Optional — Requires Same Connectivity)

> ⚠️ Only run if you can provide the clone's `DATABASE_URL` from within the cluster. The tests themselves do not create infrastructure; they connect to the DB you specify.

```bash
# From a GKE pod with the application binary or from a dev container:
# DATABASE_URL="postgresql://intent_rebase_app:REAL_PASSWORD@CLONE_PRIVATE_IP:5432/intent_rebase" \
#   cargo test -p intent-service --test migration_integration -- --ignored
# DATABASE_URL="postgresql://intent_rebase_app:REAL_PASSWORD@CLONE_PRIVATE_IP:5432/intent_rebase" \
#   cargo test -p intent-api --test webhook_integration -- --ignored
```

### Phase 3 — Cleanup (Clone Deletion)

> **Cloud SQL clones may inherit deletion protection.** If the clone was created with `deletion_protection` enabled (matching the Terraform default), you must disable it before deletion.

```bash
# 1. Check deletion protection on the clone
PROTECTION=$(gcloud sql instances describe "${NEW_INSTANCE_NAME}" \
  --project="${PROJECT}" --format="value(settings.deletionProtectionEnabled)")

if [[ "${PROTECTION}" == "True" || "${PROTECTION}" == "true" ]]; then
  echo "[$(date -Iseconds)] Disabling deletion protection on ${NEW_INSTANCE_NAME}..."
  gcloud sql instances patch "${NEW_INSTANCE_NAME}" \
    --project="${PROJECT}" \
    --no-deletion-protection
  # Wait briefly for the patch to apply
  sleep 15
fi

# 2. Delete the clone
echo "[$(date -Iseconds)] Deleting ${NEW_INSTANCE_NAME}..."
gcloud sql instances delete "${NEW_INSTANCE_NAME}" \
  --project="${PROJECT}" \
  --quiet

# 3. Verify the clone is gone
echo "[$(date -Iseconds)] Verifying deletion..."
gcloud sql instances describe "${NEW_INSTANCE_NAME}" --project="${PROJECT}" 2>&1 \
  | grep -q "NOT_FOUND" && echo "Confirmed: ${NEW_INSTANCE_NAME} deleted."
```

> **Cost warning:** The clone accumulates billing from the moment it reaches `RUNNABLE`. Delete it promptly after validation to avoid unnecessary Cloud SQL charges.

### Restore in Place (Destructive — NOT Allowed Without SRE + Budget Approval)

```bash
# ⚠️ FATAL WARNING: This overwrites the PRIMARY instance. Do NOT run without:
#   1. Explicit written SRE approval.
#   2. A scheduled maintenance window.
#   3. A verified backup/clone fallback.
#   4. A rollback plan signed off by the project owner.
# gcloud sql instances restore production-template-postgres-ed2c5bdd \
#   --project=ferrum-497801 \
#   --backup-id=BACKUP_ID
```

### Validation Checklist (To Be Completed When Executed)

| Step | Check | Expected Result | Actual Result | Pass/Fail |
|------|-------|-----------------|---------------|-----------|
| 0.1 | Preflight: auth & project | `gcloud config get-value project` = `ferrum-497801` | | |
| 0.2 | Preflight: source RUNNABLE | `gcloud sql instances describe` shows `state: RUNNABLE` | | |
| 0.3 | Preflight: backup availability | `gcloud sql backups list` returns ≥1 backup within window | | |
| 0.4 | Preflight: beta command surface | `gcloud beta sql instances clone --help` lists `--point-in-time` | | |
| 0.6 | Preflight: target name ≠ source | `NEW_INSTANCE_NAME` != `production-template-postgres-ed2c5bdd` | | |
| 1.1 | Clone command completes | `gcloud beta sql instances clone` exits 0 | | |
| 1.2 | Clone reaches RUNNABLE | `gcloud sql instances describe` shows `state: RUNNABLE` within reasonable time | | |
| 2.1 | Private-IP connectivity | `SELECT current_database()` from GKE pod or Auth Proxy succeeds | | |
| 2.2 | Schema migration metadata | `SELECT COUNT(*) FROM _sqlx_migrations` returns ≥0 (may be 0 if raw-psql migrations) | | |
| 2.3 | Intent table row count | Matches pre-restore approximate count or is non-empty | | |
| 2.4 | Core table existence | `graph_nodes`, `graph_edges`, `intent_versions`, `webhook_outbox` exist and are non-empty | | |
| 2.5 | (Optional) App smoke tests | `cargo test -p intent-service --test migration_integration` passes | | |
| 3.1 | Deletion protection disabled | Patch command exits 0 if protection was enabled | | |
| 3.2 | Clone deleted | `gcloud sql instances describe` returns `NOT_FOUND` | | |
| 6 | RPO measurement | Data loss ≤ 1 hour from target restore time | | |
| 7 | RTO measurement | Clone + validation completed ≤ 30 minutes | | |

### Forbidden Claims

| Forbidden Claim | Allowed Replacement |
|----------------|-------------------|
| `PITR restore tested on production` | `PITR clone restore validated (2026-06-18) against separate Cloud SQL clone; full production DR drill not yet executed` |
| `RPO/RTO SLA validated` | `Target RPO=1h/RTO=30m documented; validation pending execution against Cloud SQL instance` |
| `Cloud SQL backups are immutable` | `Cloud SQL automated backups enabled; immutability not equivalent to S3 Object Lock` |

### RPO/RTO Gap Analysis (2026-06-19)

**Current evidence:** Cloud SQL PITR clone restore was validated on 2026-06-18 against a separate clone (`pitr-restore-test-20260618084607`). Clone reached `RUNNABLE`, validation Job confirmed database and core tables present.

**What was NOT measured:**
- **Clone provisioning time is NOT DR RTO.** Provisioning a Cloud SQL clone from PITR was observed at >40 minutes. This is infrastructure provisioning latency, not the full incident-response RTO. RTO must include: incident declaration, decision to restore, target selection, clone provisioning, schema validation, app cutover (connection string/DNS switch), smoke tests, and service restoration.
- **RPO was not measured.** RPO requires a data-loss marker: write a timestamped/sequenced marker row to the database, trigger a restore to a point before that marker, compare the restored target against the source, and quantify how much data was lost. No such marker was written or compared.
- **Real DR scenario not executed.** No incident scenario was run: no simulated failure, no notification, no runbook execution under time pressure, no app redeployment against the restored target, no DNS cutover, no stakeholder communication.

**Suggested runbook checklist for real RPO/RTO measurement:**
1. Pre-incident: write a `dr_marker` row with `marker_id`, `created_at`, `sequence_number`.
2. Declare incident: record `incident_start_time`.
3. Choose PITR target: `restore_target_time` just before the marker.
4. Execute clone: record `clone_start_time` and `clone_ready_time`.
5. Validate: connect to clone, check `dr_marker` absent or older, check schema/tables, run app test job against clone.
6. Cutover: switch app connection string or DNS to restored target (or validate connectivity if not switching).
7. Verify: `/health` and `/ready` ok, core business path smoke test.
8. Record: `RPO = incident_start_time - last_confirmed_write_time`; `RTO = service_restored_time - incident_start_time`.
9. Cleanup: delete clone, rotate any exposed credentials.

**Conclusion:** PITR clone-only validated = infrastructure capability confirmed. Real RPO/RTO measurement = still open.

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
