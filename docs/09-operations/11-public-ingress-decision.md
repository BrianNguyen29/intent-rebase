# Public Ingress / Domain / TLS / Cloud Armor Decision

**Decision ID:** OPS-2026-06-19-001
**Date:** 2026-06-19
**Decision Maker:** BrianNguyen (Backend Lead, solo practitioner)
**Reviewer:** DuongNguyen (acknowledged as part of A-03/A-04 2026-06-15 review; 2026-06-19 assistant-generated notes pending direct reviewer confirmation)
**Status:** APPROVED

---

## Decision

**Stay private-only for initial production. Do not expose public ingress until a business need arises.**

The Intent Rebase Engine will continue to operate on an Internal LoadBalancer (`10.0.0.12`) and ClusterIP Service only. No public IP, domain, or TLS termination will be configured at this time.

---

## Rationale

| Factor | Assessment |
|--------|------------|
| Consumer requirement | No current consumer requires public access |
| Internal access | Internal LoadBalancer (`10.0.0.12`) + ClusterIP sufficient for internal consumers |
| Attack surface | Public ingress increases attack surface without business justification |
| Cost overhead | Cloud Armor / WAF adds cost and management overhead for no current benefit |
| Security gate alignment | A-04 is APPROVED WITH CONDITIONS (2026-06-15) — not production-ready; external surface expansion would require updated security signoff. Any 2026-06-19 notes are assistant-generated and require direct reviewer confirmation. |

---

## Decision Record

- **Made by:** BrianNguyen (solo practitioner)
- **Date:** 2026-06-19
- **Reviewer:** DuongNguyen (A-03/A-04 APPROVED WITH CONDITIONS on 2026-06-15 — see `docs/09-operations/10-external-review-packet.md` Section H. Any 2026-06-19 re-signoff notes are assistant-generated and pending direct reviewer confirmation; they do not constitute a direct reviewer signoff.)
- **Approval:** BrianNguyen authorized this decision as project owner and Backend Lead

---

## Prerequisites if Public Ingress is Needed Later

The following checklist must be completed **in order** before public ingress is enabled:

1. **A-04 updated security signoff** — external security review for the new external surface
2. **Domain name registered + TLS certificate** — managed certificate or cert-manager
3. **Cloud Armor / WAF policy** — evaluated and applied
4. **Public LoadBalancer or GKE Ingress** — configured with `deletion_protection = true`
5. **Network Security Policy** — applied to restrict ingress sources
6. **Sustained load test** — against the public endpoint (30+ minutes, all alert types)
7. **Runbook for public ingress management** — documented and reviewed
8. **External pentest of public surface** — completed with no HIGH/CRITICAL findings open

---

## Implementation Options if Triggered

| Option | Pattern | Use Case |
|--------|---------|----------|
| **A** | GKE Ingress with managed certificate | Simplest; single GCP-managed ingress |
| **B** | External LoadBalancer + Cloud Armor | More control; custom WAF rules |
| **C** | Cloud CDN + Cloud Armor | Global reach; edge caching if needed |

---

## Current State

| Component | Status | Detail |
|-----------|--------|--------|
| Service type | ClusterIP | `intent-api` Service, IP `34.118.227.210`, port 8080 |
| Internal LB | ✅ APPLIED | `intent-api-internal-lb`, private IP `10.0.0.12`, port `8080:31731/TCP` |
| Public ingress | 🔴 GATED | No public IP; no domain; no TLS |
| Cloud Armor | 🔴 GATED | Not evaluated |
| WAF | 🔴 GATED | Not evaluated |

> **This decision documents a private-only posture. It does not constitute a production-readiness claim.** The system is reachable by internal consumers via the Internal LoadBalancer. Public ingress is explicitly gated and will not be enabled until the prerequisites checklist is completed and a business need is documented. A-03/A-04 re-signoff has not been obtained. A-07 remains OPEN (external pen test required). Prod rotation has not been performed.

---

## Decision Caveat

> **This decision documents a private-only posture. It does NOT constitute a production-readiness claim.** The system is reachable by internal consumers via the Internal LoadBalancer. Public ingress is explicitly gated and will not be enabled until the prerequisites checklist is completed and a business need is documented. A-03/A-04 re-signoff has not been obtained. A-07 remains OPEN (external pen test required). Prod rotation has not been performed.

---

## Forbidden Claims

| Claim | Status | Why It Is Forbidden Here |
|-------|--------|--------------------------|
| `Public ingress enabled` | 🔴 GATED | Private-only by explicit decision. No public IP, no domain, no TLS, no Cloud Armor, no WAF. |
| `Production-ready external surface` | ❌ NOT CLAIMED | A-04 external security review not obtained for public surface. A-07 external pen test not executed. No domain/TLS/WAF configured. |
| `External user access` | ❌ NOT CLAIMED | Internal LoadBalancer (`10.0.0.12`) + ClusterIP only. No external consumers. |
| `TLS termination configured` | ❌ NOT CLAIMED | No certificates, no cert-manager, no managed certificates. |
| `Cloud Armor / WAF evaluated` | ❌ NOT CLAIMED | Not evaluated. Not needed for private-only. |
| `Public ingress load tested` | ❌ NOT CLAIMED | No public ingress exists. Load tests are internal ClusterIP only. |

---

## Related Documents

- `infrastructure/production/kubernetes/internal-load-balancer-service.yaml` — Internal LB manifest
- `infrastructure/production/README.md` — Ingress strategy section
- `docs/09-operations/10-external-review-packet.md` — A-03/A-04 review packet (2026-06-15 APPROVED WITH CONDITIONS; 2026-06-19 assistant-generated notes pending direct reviewer confirmation)
- `docs/10-delivery/17-production-readiness-backlog.md` — P1-3 production infrastructure status

---

*Last Updated: 2026-06-19*
