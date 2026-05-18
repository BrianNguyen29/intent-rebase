# Phase 4 — Expansion

> **Note:** The `06-` prefix in this filename is a legacy sequence number. It overlaps with `06-phase-3-batch-0-execution.md` in the same directory; the numbering is non-semantic and does not indicate ordering or dependency.

## Scope options
- policy simulation / digital twin
- memory trust linkage
- source trust registry
- advanced recommendation engine for repair choice
- marketplace/integration SDK

## Design Baselines Complete (Design-Only)

- Webhook delivery hardening — P2-6a..P2-6f design baseline complete: outbox schema, background worker lifecycle, HMAC signing + key rotation, subscription CRUD API, retry / dead-letter semantics, rollback plan. No implementation, no production readiness claim.

## Decision criteria
- khách hàng đang đau chỗ nào nhất
- data đủ để productize recommendation chưa
- adapter nào được dùng nhiều nhất
