# 06 — Threat Model v2

**Status:** Accepted Internal Planning Artifact — internal planning acceptance only; not externally reviewed
**Phase:** Phase 3  
**Owner:** Security Team

---

## Mục đích

Comprehensive threat model for Intent Rebase Engine covering:
- Attack surfaces
- Threat actors
- Attack trees
- Mitigations
- Residual risks

This is an update from the Phase 1 threat model (v1).

---

## System Overview for Threat Modeling

```
┌──────────────────────────────────────────────────────────────────┐
│                         Intent Rebase Engine                      │
├──────────────────────────────────────────────────────────────────┤
│  ┌─────────┐   ┌─────────┐   ┌─────────┐   ┌─────────┐            │
│  │ Intent  │   │ Rebase  │   │  Graph  │   │Approval │            │
│  │ Service │   │ Engine  │   │ Service │   │ Service │            │
│  └────┬────┘   └────┬────┘   └────┬────┘   └────┬────┘            │
│       │             │             │             │                  │
│  ┌────┴─────────────┴─────────────┴─────────────┴────┐           │
│  │              Data Layer (Postgres + S3)             │           │
│  └─────────────────────────────────────────────────────┘           │
│                                                                  │
│  ┌─────────┐   ┌─────────┐   ┌─────────┐   ┌─────────┐            │
│  │ Runtime │   │  Audit  │   │  Rule   │   │Console  │            │
│  │ Adapter │   │  Service│   │  Packs  │   │(Frontend│            │
│  └─────────┘   └─────────┘   └─────────┘   └─────────┘            │
└──────────────────────────────────────────────────────────────────┘
```

---

## Threat Actors

| Actor | Description | Capabilities | Intent |
|-------|-------------|--------------|--------|
| **External Attacker** | External entity attempting unauthorized access | Network access, basic exploit | Data theft, service disruption |
| **Malicious Insider** | Privileged user with bad intent | Credential access, data manipulation | Data tampering, unauthorized actions |
| **Compromised Service Account** | Service account compromised by attacker | API access, data access | Lateral movement, data exfil |
| **Rogue Agent** | AI agent acting outside intended bounds | Intent manipulation, artifact creation | Unintended side effects, bypass controls |
| **State-Sponsored Actor** | Advanced persistent threat | Significant resources, zero-days | Long-term data theft, sabotage |

---

## Attack Surfaces

### 1. API Surface

| Endpoint | Risk | Authentication |
|----------|------|----------------|
| `POST /api/v1/intents` | Intent injection | API key + JWT |
| `POST /api/v1/intents/{id}/rebase-apply` | Unauthorized rebase | API key + JWT |
| `GET /api/v1/audit/events` | Audit exfiltration | API key + JWT |
| `POST /api/v1/artifacts` | Artifact spoofing | API key + JWT |

### 2. Data Surface

| Store | Risk | Exposure |
|-------|------|----------|
| PostgreSQL | SQL injection, unauthorized access | Internal only |
| S3 | Data exfiltration, tampering | Network + credentials |
| NATS | Event injection, eavesdropping | Internal + limited external |

### 3. Runtime Surface

| Component | Risk | Impact |
|-----------|------|--------|
| Temporal | Workflow manipulation | Rebase signals intercepted |
| Runtime Adapter | Malicious adapter | Execute arbitrary actions |

### 4. Console Surface

| Component | Risk | Attack Vector |
|-----------|------|---------------|
| Next.js Frontend | XSS, CSRF | Malicious intent display, action hijacking |
| WebSocket | Real-time injection | Event stream manipulation |

---

## Attack Trees

### Attack Tree 1: Unauthorized Intent Modification

```
[Unauthorized Intent Modification]
    │
    ├── [Compromise User Credentials]
    │       ├── Phishing
    │       │       └── Mitigated by: MFA, security awareness
    │       ├── Credential Stuffing
    │       │       └── Mitigated by: rate limiting, detection
    │       └── Keylogger
    │               └── Mitigated by: endpoint security
    │
    ├── [Exploit API Vulnerability]
    │       ├── SQL Injection
    │       │       └── Mitigated by: parameterized queries, WAF
    │       ├── Broken Authentication
    │       │       └── Mitigated by: JWT validation, API key rotation
    │       └── IDOR (Insecure Direct Object Reference)
    │               └── Mitigated by: authorization checks, tenant isolation
    │
    └── [Compromise Service Account]
            ├── Credential Exposure
            │       └── Mitigated by: secret rotation, vault
            └── Privilege Escalation
                    └── Mitigated by: least privilege, RBAC
```

### Attack Tree 2: Audit Trail Tampering

```
[Audit Trail Tampering]
    │
    ├── [Delete Audit Events]
    │       └── Mitigated by: no DELETE permission, trigger protection
    │
    ├── [Modify Audit Events]
    │       ├── SQL UPDATE
    │       │       └── Mitigated by: no UPDATE permission, trigger protection
    │       └── S3 Object Modification
    │               └── Mitigated by: S3 Object Lock, hash chain verification
    │
    └── [Break Hash Chain]
            ├── Modify Past Event
            │       └── Mitigated by: hash chain detection
            └── Insert Fake Event
                    └── Mitigated by: sequence verification
```

### Attack Tree 3: Approval Bypass

```
[Approval Bypass]
    │
    ├── [Manipulate Intent to Avoid Approval Requirement]
    │       ├── Change classification to "low risk"
    │       │       └── Mitigated by: rule pack validation, monitoring
    │       └── Exclude critical fields from diff
    │               └── Mitigated by: mandatory field list
    │
    ├── [Spoof Approval]
    │       ├── Steal approver credentials
    │       │       └── Mitigated by: MFA, session binding
    │       └── Manipulate approval workflow
    │               └── Mitigated by: approval audit trail
    │
    └── [Exploit Policy Snapshot]
            └── Tamper with policy snapshot
                    └── Mitigated by: S3 Object Lock, snapshot verification
```

---

## Mitigations

### High Priority

| Threat | Mitigation | Priority |
|--------|------------|----------|
| Unauthorized intent modification | RBAC + MFA + API key rotation | P0 |
| Audit tampering | Append-only + hash chain + S3 Object Lock | P0 |
| Approval bypass | Policy snapshots + multi-party approval | P0 |
| Data exfiltration | Tenant isolation + network segmentation | P0 |

### Medium Priority

| Threat | Mitigation | Priority |
|--------|------------|----------|
| Runtime adapter compromise | Adapter signing + verification | P1 |
| Console XSS | Content Security Policy + sanitization | P1 |
| Service account compromise | Secret rotation + vault + monitoring | P1 |

### Lower Priority

| Threat | Mitigation | Priority |
|--------|------------|----------|
| CSRF on console | CSRF tokens + SameSite cookies | P2 |
| Rate limiting bypass | API gateway + per-tenant limits | P2 |

---

## Security Controls Mapping

| Control | Implementation |
|---------|----------------|
| **Identification** | JWT + API keys + RBAC |
| **Authentication** | OIDC + MFA for privileged users |
| **Authorization** | RBAC + tenant isolation + least privilege |
| **Confidentiality** | TLS in transit + encryption at rest |
| **Integrity** | Hash chains + S3 Object Lock + audit |
| **Availability** | HA deployment + SLOs + runbooks |
| **Non-repudiation** | Audit trail + policy snapshots |

---

## Residual Risks

The Phase 2b residual risk register has been populated with entries RR-04 through RR-10, sourced from the [08 — Phase 2b Security Findings Input](../10-delivery/08-phase-2b-security-findings-input.md). Key risks carried forward into Phase 3 include:

- **RR-04:** Event delivery failure detection latency (JetStream/DLQ not yet production-ready)
- **RR-05:** Operator notification not actually delivered (external delivery semantics pending)
- **RR-06:** Artifact custody is metadata-only (actual storage movement deferred)
- **RR-07:** Forensic replay bounded to cooperative checkpoint (not full runtime reset)
- **RR-08:** Snapshot evidence integrity under degraded event payloads
- **RR-09:** Cross-tenant data exposure through incomplete enforcement (P3 Batch 3a priority)
- **RR-10:** Moving trust boundaries during Phase 3 early state

See [13 — Residual Risk Specification](./13-residual-risk-spec.md) for full tracking of accepted risks.

---

## Related Documents

- [07 — Authorization Matrix](./07-authz-matrix.md)
- [08 — Tenant Isolation](./08-tenant-isolation.md)
- [13 — Residual Risk Specification](./13-residual-risk-spec.md)
