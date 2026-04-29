# Staging Environment — Docker Compose Scaffold

> **Status:** `NON-PRODUCTION — Staging scaffold for local validation and staging-like evidence collection`
>
> **Evidence Strength:** LOCAL DOCKER-COMPOSE (staging-like) — NOT equivalent to production or external staging environment

---

## Purpose

This directory provides a docker-compose-based staging scaffold for:

1. Local validation of staging-like deployment characteristics
2. Staging evidence collection (L3 staging-like load tests, observability validation)
3. Development of operational procedures before production

**This is NOT production.** The staging scaffold requires the following external gates to pass before any production consideration:

- External SRE sign-off
- External security review / pen test
- External load testing (L3+)
- Compliance checklist completion

---

## Quick Start

### 1. Prerequisites

- Docker and Docker Compose installed
- `cd infrastructure/staging/`

### 2. Setup Environment

```bash
# Copy the env template
cp .env.example .env

# Edit .env and replace all CHANGE_ME values with staging-appropriate credentials
# Required: STAGING_POSTGRES_PASSWORD, STAGING_MINIO_ROOT_PASSWORD, STAGING_GRAFANA_PASSWORD
```

### 3. Start Staging Stack

```bash
# Start core services (postgres, nats, minio)
docker compose up -d

# Verify services are healthy
docker compose ps

# Start with observability stack (optional)
docker compose --profile observability up -d
```

### 4. Verify Services

```bash
# Postgres
docker compose exec postgres pg_isready -U intent_rebase_staging

# NATS
curl -s http://localhost:8223/healthz

# MinIO
# Web console: http://localhost:9003 (user: staging_minioadmin, pass: from .env)
# S3 API: http://localhost:9002
```

### 5. Stop Staging Stack

```bash
docker compose down

# To also remove volumes (WARNING: destroys all staging data)
docker compose down -v
```

---

## Service Ports

| Service | Local Dev Port | Staging Port | Container Name |
|---------|---------------|--------------|----------------|
| Postgres | 5432 | 5433 | intent-rebase-staging-postgres |
| NATS | 4222 | 4223 | intent-rebase-staging-nats |
| NATS Monitor | 8222 | 8223 | intent-rebase-staging-nats |
| MinIO API | 9000 | 9002 | intent-rebase-staging-minio |
| MinIO Console | 9001 | 9003 | intent-rebase-staging-minio |

### Observability Stack (--profile observability)

| Service | Local Dev Port | Staging Port | Container Name |
|---------|---------------|--------------|----------------|
| intent-api | 8080 | 8081 | intent-rebase-staging-api-metrics |
| Prometheus | 9090 | 9091 | intent-rebase-staging-prometheus |
| Alertmanager | 9093 | 9094 | intent-rebase-staging-alertmanager |
| Grafana | 3000 | 3001 | intent-rebase-staging-grafana |

---

## Validation Commands

### Compose Config Validation

```bash
# Validate compose file syntax
docker compose -f infrastructure/staging/docker-compose.yml config

# Validate with full output
docker compose -f infrastructure/staging/docker-compose.yml config --quiet || echo "Config errors detected"
```

### Health Checks

```bash
# Check all service health
docker compose -f infrastructure/staging/docker-compose.yml ps

# Check specific service
docker compose -f infrastructure/staging/docker-compose.yml exec postgres pg_isready -U intent_rebase_staging
docker compose -f infrastructure/staging/docker-compose.yml exec nats nats server check jetstream
```

---

## Environment Variables

| Variable | Description | Required | Default |
|----------|-------------|----------|---------|
| `STAGING_POSTGRES_PASSWORD` | Postgres password | Yes | (none — must be set) |
| `STAGING_MINIO_ROOT_USER` | MinIO root user | Yes | staging_minioadmin |
| `STAGING_MINIO_ROOT_PASSWORD` | MinIO root password | Yes | (none — must be set) |
| `STAGING_GRAFANA_USER` | Grafana admin user | No | admin |
| `STAGING_GRAFANA_PASSWORD` | Grafana admin password | No | staging_admin_change_me |
| `STAGING_MODE` | Enable staging mode | No | true |

---

## Relationship to Other Environments

```
local (docker-compose)  -->  staging (docker-compose, staging-like)  -->  production
     |                              |
     | local dev                    | staging evidence collection
     | port 5432, 4222, 9000        | port offsets (5433, 4223, 9002)
     | no resource limits          | informational resource limits
     |                              |
     |                              +-- requires SRE/security/load/pen gates
```

**Note:** When referring to the local docker-compose environment, do NOT call it "staging." Use "docker-compose local" or "local development environment."

---

## Forbidden Claims

This staging scaffold does NOT constitute:

- Production-ready infrastructure
- Remote CI validation
- External SRE or security sign-off
- Production-equivalent performance or reliability

Do not represent this scaffold as anything other than a local validation tool for staging-like evidence collection.

---

## File Structure

```
infrastructure/staging/
├── docker-compose.yml   # Staging compose configuration
├── .env.example         # Environment variable template
└── README.md           # This file
```

---

## Relationship to Phase 3 Documentation

This staging scaffold is referenced in:

- `docs/09-operations/01-environments.md` — Environment inventory
- `docs/09-operations/02-ci-cd.md` — CI/CD pipeline (staging deploy step)
- `docs/10-delivery/16-solo-ops-evidence-plan.md` — Phase B-2 staging-like evidence collection

**Status Tracking:** This scaffold is a **documentation-only milestone**. Its existence is recorded, but it does not imply any external gate has passed.
