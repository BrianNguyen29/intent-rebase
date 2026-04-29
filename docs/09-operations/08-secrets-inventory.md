# 08 — Secrets Inventory & Rotation Procedure

**Status:** `DOCUMENTED — Inventory Template & Rotation Procedure Templates Only`
**Phase:** Phase 3 — Ops Evidence Track
**Owner:** Backend Lead (solo practitioner)
**Last Updated:** April 2026

---

## Purpose

This document provides a **secrets inventory template** and **rotation procedure templates** for the Intent Rebase Engine. It documents the current state of known secrets, their intended rotation cadences, and the manual procedures for rotation. These are **documentation templates only** — no live secrets have been rotated in production.

> **⚠️ Evidence Strength Disclaimer**
>
> This document catalogs **known secret locations and rotation procedures**. It does not confirm that secrets have been rotated or that rotation has been validated. Live secret rotation requires production infrastructure and coordination with external SRE/security teams.

---

## Secrets Inventory

### Inventory Status

| Category | Secrets Known | Rotation Implemented | Rotation Validated |
|----------|-------------|---------------------|-------------------|
| Database credentials | ✅ Yes | 🟡 Template only | ❌ No |
| NATS credentials | ✅ Yes | 🟡 Template only | ❌ No |
| MinIO/S3 credentials | ✅ Yes | 🟡 Template only | ❌ No |
| API keys (tenant) | ✅ Yes | 🟡 Template only | ❌ No |
| JWT signing keys | ✅ Yes | 🟡 Template only | ❌ No |
| TLS certificates | 🟡 Partial | 🟡 Template only | ❌ No |
| Encryption keys (at-rest) | 🟡 Partial | ❌ Not documented | ❌ No |

---

## Secret Categories

### 1. Database Credentials

| Secret | Location | Used By | Rotation Cadence | Current Status |
|--------|----------|---------|-----------------|----------------|
| `DATABASE_URL` (Postgres) | Environment variable / `.env` | intent-api, rebase-engine, graph-service | 90 days | Template — not rotated |
| `POSTGRES_PASSWORD` | `.env` / secrets manager | PostgreSQL auth | 90 days | Template — not rotated |
| Read replica credentials | `.env` / secrets manager | Read-only queries | 90 days | Template — not rotated |

**Secret Storage:**
- **Local dev:** `.env` file (not committed to git)
- **Staging/Production:** HashiCorp Vault / AWS Secrets Manager

**Rotation Procedure Template:**

```bash
#!/bin/bash
# rotate-postgres-secret.sh — PostgreSQL Credential Rotation Template
# Cadence: 90 days
# Downtime: Zero (use connection pool draining)

set -euo pipefail

# 1. Generate new password
NEW_PASSWORD=$(openssl rand -base64 32)
echo "New password generated at $(date -Iseconds)"

# 2. Update secrets manager (Vault example)
# VAULT_ADDR="https://vault.internal:8200"
# vault kv put secret/intent-rebase/postgres password="${NEW_PASSWORD}"

# 3. Update application environment (no restart needed if using connection pool)
# The application should read secrets from environment or secrets manager
# export DATABASE_PASSWORD="${NEW_PASSWORD}"

# 4. For PostgreSQL native auth: ALTER USER
# psql -c "ALTER USER intent_rebase WITH PASSWORD '${NEW_PASSWORD}';"

# 5. Verify connectivity
# psql -c "SELECT 1;"

# 6. Prune old versions in secrets manager (after verification)
# vault kv delete secret/intent-rebase/postgres -versions=1

echo "[$(date -Iseconds)] PostgreSQL rotation complete. Verify all services."
```

---

### 2. NATS Credentials

| Secret | Location | Used By | Rotation Cadence | Current Status |
|--------|----------|---------|-----------------|----------------|
| `NATS_CREDS` (user credentials) | `.env` / secrets manager | intent-api (JetStream) | 180 days | Template — not rotated |
| `NATS_USERNAME` | `.env` / secrets manager | NATS authentication | 180 days | Template — not rotated |
| `NATS_PASSWORD` | `.env` / secrets manager | NATS authentication | 180 days | Template — not rotated |

**Rotation Procedure Template:**

```bash
#!/bin/bash
# rotate-nats-secret.sh — NATS Credential Rotation Template
# Cadence: 180 days
# Downtime: Minimal (JetStream handles reconnection)

set -euo pipefail

NATS_HOST="${NATS_HOST:-localhost}"
NATS_PORT="${NATS_PORT:-4222}"
NATS_ADMIN_USER="${NATS_ADMIN_USER:-admin}"
NATS_ADMIN_PASS="${NATS_ADMIN_PASS:-}"

# 1. Generate new credentials
NEW_USER="intent_rebase_$(date +%Y%m%d)"
NEW_PASSWORD=$(openssl rand -base64 24)
echo "[$(date -Iseconds)] New NATS user: ${NEW_USER}"

# 2. Create new user in NATS (requires admin credentials)
# nats --server "nats://${NATS_HOST}:${NATS_PORT}" \
#   --user "${NATS_ADMIN_USER}" --password "${NATS_ADMIN_PASS}" \
#   server user add "${NEW_USER}" --password "${NEW_PASSWORD}"

# 3. Grant JetStream permissions
# nats --server "nats://${NATS_HOST}:${NATS_PORT}" \
#   --user "${NATS_ADMIN_USER}" --password "${NATS_ADMIN_PASS}" \
#   server user update "${NEW_USER}" --password "${NEW_PASSWORD}" \
#   --issuer "ire-system" --subjects "audit.events.>"

# 4. Update secrets manager
# vault kv put secret/intent-rebase/nats \
#   username="${NEW_USER}" \
#   password="${NEW_PASSWORD}"

# 5. Verify new credentials work
# nats --server "nats://${NATS_HOST}:${NATS_PORT}" \
#   --user "${NEW_USER}" --password "${NEW_PASSWORD}" \
#   stream ls

# 6. Remove old user (after verification)
# nats --server "nats://${NATS_HOST}:${NATS_PORT}" \
#   --user "${NATS_ADMIN_USER}" --password "${NATS_ADMIN_PASS}" \
#   server user rm "${OLD_USER}"

echo "[$(date -Iseconds)] NATS credential rotation complete."
```

---

### 3. MinIO/S3 Credentials

| Secret | Location | Used By | Rotation Cadence | Current Status |
|--------|----------|---------|-----------------|----------------|
| `MINIO_ROOT_USER` | `.env` / secrets manager | MinIO console, S3 API | 180 days | Template — not rotated |
| `MINIO_ROOT_PASSWORD` | `.env` / secrets manager | MinIO console, S3 API | 180 days | Template — not rotated |
| `AWS_ACCESS_KEY_ID` (app) | `.env` / secrets manager | intent-api S3 client | 90 days | Template — not rotated |
| `AWS_SECRET_ACCESS_KEY` (app) | `.env` / secrets manager | intent-api S3 client | 90 days | Template — not rotated |

**Rotation Procedure Template:**

```bash
#!/bin/bash
# rotate-minio-secret.sh — MinIO/S3 Credential Rotation Template
# Cadence: 90-180 days
# Downtime: Zero (S3 client handles reconnection)

set -euo pipefail

MINIO_ENDPOINT="${MINIO_ENDPOINT:-localhost:9000}"
NEW_ACCESS_KEY="ire_app_$(date +%Y%m%d)"
NEW_SECRET_KEY=$(openssl rand -base64 32)

echo "[$(date -Iseconds)] New MinIO access key: ${NEW_ACCESS_KEY}"

# 1. Create new MinIO user via mc admin
# mc admin user add local "${NEW_ACCESS_KEY}" "${NEW_SECRET_KEY}"

# 2. Attach policy to new user
# mc admin policy attach local readwrite --user "${NEW_ACCESS_KEY}"

# 3. Update secrets manager
# vault kv put secret/intent-rebase/minio \
#   access_key="${NEW_ACCESS_KEY}" \
#   secret_key="${NEW_SECRET_KEY}"

# 4. Verify new credentials
# mc ls local/ire-policy-snapshots/

# 5. Disable old user (after verification window)
# mc admin user disable local "${OLD_ACCESS_KEY}"

echo "[$(date -Iseconds)] MinIO credential rotation complete."
```

---

### 4. API Keys (Tenant)

| Secret | Location | Used By | Rotation Cadence | Current Status |
|--------|----------|---------|-----------------|----------------|
| Per-tenant API keys | Database (`api_keys` table) | Tenant clients | Per-tenant policy | Template — per-key |
| JWT signing keys | Environment / secrets manager | JWT issuance | 365 days | Template — not rotated |

**Tenant API Key Rotation Procedure Template:**

```bash
#!/bin/bash
# rotate-tenant-api-key.sh — Tenant API Key Rotation Template
# Cadence: Per-tenant policy (default: 90 days)
# Downtime: Zero (dual-key window)

set -euo pipefail

TENANT_ID="${TENANT_ID:-}"
OLD_KEY_ID="${OLD_KEY_ID:-}"
NEW_KEY_ID="key_$(openssl rand -hex 16)"
NEW_SECRET=$(openssl rand -base64 32)

echo "[$(date -Iseconds)] Rotating API key for tenant: ${TENANT_ID}"

# 1. Generate new key pair (both key_id and secret)
# Store hash of secret in database
# INSERT INTO api_keys (key_id, tenant_id, secret_hash, created_at, expires_at)
# VALUES ('${NEW_KEY_ID}', '${TENANT_ID}', sha256('${NEW_SECRET}'), NOW(), NOW() + interval '90 days');

# 2. Return new secret to tenant (out-of-band)
# DO NOT log the secret in plaintext

# 3. Old key still valid during dual-window (e.g., 24 hours)
# UPDATE api_keys SET revoked_at = NOW() + interval '24 hours' WHERE key_id = '${OLD_KEY_ID}';

# 4. After dual-window: hard revoke old key
# UPDATE api_keys SET revoked_at = NOW() WHERE key_id = '${OLD_KEY_ID}';

echo "[$(date -Iseconds)] Tenant API key rotation complete. Key ID: ${NEW_KEY_ID}"
```

**JWT Signing Key Rotation Procedure Template:**

```bash
#!/bin/bash
# rotate-jwt-signing-key.sh — JWT Signing Key Rotation Template
# Cadence: 365 days
# Downtime: Zero (support old and new keys during transition)

set -euo pipefail

JWT_PRIVATE_KEY_PATH="${JWT_PRIVATE_KEY_PATH:-/secrets/jwt-private.pem}"
JWT_PUBLIC_KEY_PATH="${JWT_PUBLIC_KEY_PATH:-/secrets/jwt-public.pem}"
TRANSITION_PERIOD_HOURS="${TRANSITION_PERIOD_HOURS:-24}"

# 1. Generate new key pair
openssl ecparam -name prime256v1 -genkey -noout -out "/tmp/jwt-new-private.pem"
openssl ec -in "/tmp/jwt-new-private.pem" -pubout -out "/tmp/jwt-new-public.pem"

# 2. Store both old and new public keys during transition
# The JWT library should support multiple valid issuers
# jwt.verify(token, [old_public_key, new_public_key])

# 3. Update secrets manager with new private key
# vault kv put secret/intent-rebase/jwt-private key=@/tmp/jwt-new-private.pem

# 4. Restart services to pick up new key
# systemctl restart intent-api

# 5. After transition period: remove old key from allowed list
# Update JWT library config to use only new public key

# 6. Rotate old key to "old" category for emergency rollback
# mv /secrets/jwt-private.pem /secrets/jwt-private-old.pem
# mv /secrets/jwt-public.pem /secrets/jwt-public-old.pem

echo "[$(date -Iseconds)] JWT signing key rotation complete."
```

---

### 5. TLS Certificates

| Secret | Location | Used By | Rotation Cadence | Current Status |
|--------|----------|---------|-----------------|----------------|
| Server TLS cert/key | `/etc/ssl/certs/`, `/etc/ssl/private/` | intent-api HTTPS | 90 days (Let's Encrypt) or 365 days | Template — not rotated |
| Client TLS certs | Secrets manager | Service-to-service mTLS | 365 days | Template — not rotated |

**TLS Certificate Rotation Procedure Template:**

```bash
#!/bin/bash
# rotate-tls-cert.sh — TLS Certificate Rotation Template
# Cadence: 90 days (Let's Encrypt) or 365 days (commercial CA)
# Downtime: Zero (reload without restart)

set -euo pipefail

CERT_PATH="${CERT_PATH:-/etc/ssl/certs/intent-api.crt}"
KEY_PATH="${KEY_PATH:-/etc/ssl/private/intent-api.key}"
CERT_CHAIN_PATH="${CERT_PATH:-/etc/ssl/certs/intent-api-chain.crt}"

# 1. Obtain new certificate (Let's Encrypt example)
# certbot certonly --nginx -d api.intent-rebase.example.com --renew-by-default

# 2. Or: Copy manually-provided certificate
# cp /tmp/new-cert.crt "${CERT_PATH}"
# cp /tmp/new-cert.key "${KEY_PATH}"

# 3. Verify certificate
# openssl x509 -in "${CERT_PATH}" -noout -dates
# openssl x509 -in "${CERT_PATH}" -noout -subject

# 4. Reload nginx/apache/t intent-api to pick up new cert (no restart)
# nginx -s reload
# systemctl reload intent-api

# 5. Verify new cert is being served
# echo | openssl s_client -connect localhost:8080 2>/dev/null | openssl x509 -noout -dates

echo "[$(date -Iseconds)] TLS certificate rotation complete."
```

---

## Secret Rotation Cadence Summary

| Secret Category | Rotation Cadence | Zero-Downtime Support | Notes |
|-----------------|-----------------|----------------------|-------|
| PostgreSQL credentials | 90 days | ✅ Yes (pool draining) | Rotate password, not connection string |
| NATS credentials | 180 days | ✅ Yes (JetStream reconnect) | Keep old user during transition |
| MinIO/S3 credentials | 90-180 days | ✅ Yes (S3 client reconnect) | Keep old user during transition |
| Tenant API keys | 90 days (per-tenant) | ✅ Yes (dual-key window) | 24-hour overlap recommended |
| JWT signing keys | 365 days | ✅ Yes (dual-key window) | 24-hour overlap recommended |
| TLS certificates | 90-365 days | ✅ Yes (reload, no restart) | Use `systemctl reload`, not restart |

---

## Secrets Management Tooling

### Supported Tools

| Tool | Use Case | Phase 3 Status |
|------|----------|----------------|
| HashiCorp Vault | Production secrets management | 🟡 Template only |
| AWS Secrets Manager | Production (AWS-hosted) | 🟡 Template only |
| Kubernetes Secrets | In-cluster secrets | 🟡 Template only |
| `.env` files | Local development | ✅ Documented |
| `mc` (MinIO client) | MinIO admin operations | 🟡 Template only |

### Vault Integration Template

```bash
# vault-template.sh — HashiCorp Vault Integration Template
# Phase 3: Template only — no actual Vault deployment

VAULT_ADDR="${VAULT_ADDR:-https://vault.internal:8200}"
SECRET_PATH="secret/intent-rebase"

# Read secret
# vault kv get "${SECRET_PATH}/postgres"

# Write secret
# vault kv put "${SECRET_PATH}/postgres" \
#   username="intent_rebase" \
#   password="$(openssl rand -base64 32)"

# Read with token
# VAULT_TOKEN="..." vault kv get "${SECRET_PATH}/nats"

# Enable transit encryption (for application-level encryption)
# vault secrets enable transit
# vault write transit/keys/intent-rebase type="rsa-4096"
```

---

## Deferred Items (Phase 4+)

| Item | Reason Deferred | Phase |
|------|----------------|-------|
| Vault deployment/integration | Requires external secrets infra | Phase 4+ |
| Automated rotation (automatic, not scripted) | Requires Vault rotation config | Phase 4+ |
| Encryption key (at-rest) for PostgreSQL | Requires KMS integration | Phase 4+ |
| Secret audit logging | Requires Vault audit device | Phase 4+ |

---

## Forbidden Claims

| Forbidden Claim | Allowed Replacement |
|----------------|-------------------|
| `Secrets have been rotated` | `Secrets rotation procedures documented; actual rotation requires production execution` |
| `Secrets management is production-ready` | `Secrets inventory and rotation templates documented; tooling not deployed` |
| `Zero-downtime rotation verified` | `Rotation templates designed for zero-downtime; verification requires production` |

---

## Relationship to Other Documents

| Document | Relationship |
|----------|--------------|
| `docs/09-operations/07-backup-restore.md` | Backup procedures must include secrets rotation schedule |
| `docs/08-security/06-pen-test-scope.md` | Pen test should include credential theft scenario |
| `docs/09-operations/03-observability.md` | Secrets access should emit audit events |

---

## Update Log

| Date | Updated By | Changes |
|------|------------|---------|
| April 2026 | (fixer) | Initial creation — secrets inventory (Postgres, NATS, MinIO, API keys, JWT, TLS); rotation procedure templates; tooling notes; deferred items |
