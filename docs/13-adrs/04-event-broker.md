# ADR-04 — Event Broker Selection

**Status:** Proposed (NATS recommended if no platform bias)  
**Date:** 2026-04-03  
**Authors:** Intent Rebase Engine Team  
**Phase:** Phase 0–P1  

---

## Context

IRE requires an event bus for:
- **Internal event streaming** — audit events, rebase notifications, artifact lifecycle events
- **Runtime signal propagation** — distributing rebase signals from API to runtime adapters
- **Async job processing** — queueing rebase computations, graph propagation tasks

Options considered:
- **NATS JetStream** — lightweight, high-performance, strong durability guarantees
- **Apache Kafka** — enterprise-grade, rich ecosystem, complex operational requirements
- **PostgreSQL LISTEN/NOTIFY** — simple, no extra infrastructure, limited fan-out

---

## Decision

**NATS JetStream as primary event broker, unless organizational platform bias favors Kafka.**

### Rationale

1. **Operational simplicity** — NATS is lightweight, single binary, minimal configuration vs Kafka's complex cluster setup
2. **Performance** — NATS handles high message throughput with lower latency
3. **Durability** — JetStream provides persistence, at-least-once delivery, and message retention policies
4. **Polyglot support** — NATS clients available in all major languages including Rust
5. **No platform bias** — if IRE is deployed on a Kafka-heavy org (Confluent, AWS MSK), Kafka may be preferred; this ADR records the analysis

### NATS JetStream Configuration

```
Streams:
  - name: AUDIT_EVENTS
    subjects: ["audit.>"]
    retention: limits (immutable, until retention policy)
    max_BYTES: 100GB (per tenant, configurable)

  - name: REBASE_SIGNALS
    subjects: ["rebase.signal.>"]
    retention: interest
    max_MSGS: 1000000

  - name: ARTIFACT_EVENTS
    subjects: ["artifact.>"]
    retention: limits
    max_BYTES: 50GB (per tenant)
```

### Fallback: Kafka

If the deployment platform has existing Kafka infrastructure:
- Use Kafka topics with retention=7days minimum for audit, 1day for signals
- Avoid Kafka Streams (use IRE's own processing logic)
- `confluent-kafka-rust` for Rust client

---

## Consequences

### Positive
- NATS JetStream provides required durability without Kafka operational overhead
- Subject-based routing enables fine-grained event filtering
- Horizontal scaling via JetStream consumers

### Negative
- NATS JetStream requires dedicated cluster (vs PostgreSQL LISTEN/NOTIFY which needs nothing extra)
- No schema registry; event schema versioning must be managed by IRE

### Neutral
- Phase 0: local NATS container for development
- Phase 3: production NATS cluster with TLS and authentication

---

## Implementation Notes

- `nats.rs` client with JetStream support
- Event schema versioning: `v1` prefix in subject, e.g., `audit.events.v1.>`, migration path to `v2` defined
- Dead-letter queue: `DLQ` stream for failed message delivery
- Consumer groups for horizontal scaling

---

## Related ADRs

- [ADR-02](./02-data-plane.md) — Storage architecture
- [ADR-05](./05-observability-baseline.md) — Observability event handling

---

## References

- NATS JetStream: https://docs.nats.io/nats-concepts/jetstream
- NATS Rust client: https://github.com/nats-io/nats.rs