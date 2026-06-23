# NATS JetStream — Hardening Artifacts and Future Roadmap

> **Status:** Single-node pilot deployed and validated. These artifacts are **optional / future-hardening only**. Do not apply them without owner approval and SRE review (A-03).
> **Current:** `nats-statefulset.yaml` (1 replica, no TLS, no HA, token auth via shell substitution). App consumer (`CheckpointCreatorConsumer`) enabled behind env gate.
> **Scope:** Private-only. No production-ready claim.

---

## Current State (Pilot)

| Property | Value | Production Hard Requirement |
|----------|-------|----------------------------|
| Replicas | 1 | ≥ 3 for HA |
| Clustering | No | Yes (anti-affinity, headless service) |
| TLS | No | Yes (cert-manager or managed CA) |
| mTLS | No | Yes for multi-tenant |
| Auth | Token (shell substitution) | JWT accounts or TLS client certs |
| ACLs | No | Yes (per-tenant stream isolation) |
| Per-tenant streams | No | ADR-15 staged migration |
| Prometheus | Sidecar exporter (7777) | Cluster-wide monitoring or ServiceMonitor |

---

## Forbidden Claims

| Claim | Status | Why It Is Forbidden Here |
|-------|--------|--------------------------|
| `NATS TLS enabled` | ❌ NOT CLAIMED | No TLS/mTLS on pilot. Token auth only via shell substitution. No cert-manager or CA. |
| `NATS HA cluster` | ❌ NOT CLAIMED | Single-node StatefulSet (1 replica). No clustering, no anti-affinity, no headless service. |
| `NATS per-tenant streams` | ❌ NOT CLAIMED | ADR-15 design-only. No implementation, no migration, no ACLs. |
| `NATS production-grade` | ❌ NOT CLAIMED | Pilot only. App consumer env-gated but not validated under production load. No SRE signoff for production NATS. |

---

## Optional Hardening Manifests (Do Not Apply Without Approval)

| File | Purpose | Risk If Applied |
|------|---------|-----------------|
| `nats-config-ha.yaml` | 3-node cluster config with routes and clustering | Breaks existing single-node pilot; requires headless service, anti-affinity, multiple PVCs |
| `nats-tls-config.yaml` | TLS/mTLS with cert-manager or mounted certificates | Requires cert generation or CA; breaks clients that do not expect TLS |
| `nats-statefulset.yaml` (current) | Single-node pilot | No risk; already applied and validated |

---

## Future-Hardening Checklist (A-03 Gated)

1. **TLS**:
   - Generate or provision certificates (cert-manager, Google-managed CA, or self-signed for internal use)
   - Update `nats.conf` with `tls` block (`cert_file`, `key_file`, `ca_file`)
   - Update `nats-statefulset.yaml` to mount cert Secrets
   - Update app `NATS_URL` from `nats://` to `tls://`
   - Validate with `nats server check` and client connection test

2. **HA Cluster**:
   - Change `replicas: 1` → `replicas: 3` in `nats-statefulset.yaml`
   - Add `podAntiAffinity` to spread across nodes
   - Change Service to `ClusterIP: None` (headless) for stable DNS per pod
   - Add `routes` in `nats.conf` pointing to `nats-0.nats:6222`, `nats-1.nats:6222`, `nats-2.nats:6222`
   - Add `cluster` block with `listen: 0.0.0.0:6222`, `name: nats-cluster`
   - Add `cluster` port to StatefulSet and Service
   - Validate with `nats server check cluster` and stream replication test

3. **Per-Tenant Streams (ADR-15)**:
   - Review `docs/13-adrs/15-nats-per-tenant-streams.md`
   - Execute Stage 1 (readiness): ensure app supports per-tenant stream creation
   - Execute Stage 2 (pilot tenant): create one tenant-specific stream, validate idempotent `get_or_create_stream`
   - Execute Stage 3 (rollout): migrate remaining tenants with duplicate storage guardrail
   - Execute Stage 4 (cleanup): remove global stream after all tenants migrated
   - Add ACLs per tenant (NATS `accounts` + `permissions` or JetStream user-level limits)

4. **Auth Hardening**:
   - Replace shell-substituted token auth with NATS `accounts` + JWT or TLS client certs
   - Add `authorization` per-account with `permissions` (publish/subscribe allow/deny lists)
   - Rotate NATS credentials via GSM + ESO

5. **Monitoring**:
   - Add `nats` ServiceMonitor if Prometheus Operator is used, or keep static target in `prometheus-config.yaml`
   - Add `kube-state-metrics` for StatefulSet-level metrics (replicas, ready, current)
   - Add `node-exporter` for node-level metrics (CPU, memory, disk pressure)
   - Define resource-based alerts (CPU > 80%, memory > 80%, disk > 85%)

---

## Related Documents

- `docs/13-adrs/15-nats-per-tenant-streams.md` — ADR-15 per-tenant stream design
- `docs/09-operations/12-authorization-signoff-packet.md` — Gate 5 (NATS) sign-off status
- `infrastructure/production/kubernetes/prometheus-config.yaml` — Prometheus scrape targets (includes `nats` static target)
- `infrastructure/production/kubernetes/nats/nats-statefulset.yaml` — Current single-node pilot
- `infrastructure/production/kubernetes/nats/nats-config.yaml` — Current single-node config

---

*Last Updated: 2026-06-23*
