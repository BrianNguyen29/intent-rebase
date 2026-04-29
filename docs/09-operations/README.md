# 09-operations — Non-production Operations Pack

> **Status:** Integration-ready / Non-production
> Procedures and templates provided for development and integration testing. Production deployment requires Phase 3 exit gate and SRE/security sign-off.

## Overview

This pack provides **procedure templates and plans** for operations evidence collection — not executed production evidence. Templates support Phase 3 exit gate and external review prep.

## Contents

### Environments & Deployment
- [01-environments.md](./01-environments.md) — Environment definitions (dev, staging, production)
- [02-ci-cd.md](./02-ci-cd.md) — CI/CD pipelines and deployment procedures

### Observability
- [03-observability.md](./03-observability.md) — Observability stack (metrics, logs, traces)
- [04-sre-and-slos.md](./04-sre-and-slos.md) — SLO/SLI definitions and error budget policies
- [06-slo-dashboard.md](./06-slo-dashboard.md) — SLO dashboard templates

### Runbooks & Tenant Management
- [05-runbooks.md](./05-runbooks.md) — Operational runbooks for common procedures
- [06-tenant-onboarding.md](./06-tenant-onboarding.md) — Tenant onboarding procedures

### Production-Hardening Evidence Templates
- [07-backup-restore.md](./07-backup-restore.md) — Backup & restore playbooks (RPO=1h, RTO=30m targets)
- [08-secrets-inventory.md](./08-secrets-inventory.md) — Secrets inventory and rotation templates
- [09-observability-evidence-checklist.md](./09-observability-evidence-checklist.md) — Evidence collection for Prometheus, Grafana, Alertmanager, traces
- [10-external-review-packet.md](./10-external-review-packet.md) — Template for requesting external SRE and security review
- [11-pen-load-test-packet.md](./11-pen-load-test-packet.md) — Pen test and load test templates (L1/L2 local; L3-L5 pending)

## Usage

These are **templates and plans** for evidence collection. They support Phase 3 exit gate preparation and are not intended as executed production procedures.

For project status, see [10-delivery/00-current-status.md](../10-delivery/00-current-status.md).
