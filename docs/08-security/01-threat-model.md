# Threat Model

## Assets
- intent versions
- approvals
- artifact provenance
- side effect ledger
- audit events
- forensic exports
- tenant secrets / tokens

## Adversaries
- external attacker spoofing source changes
- malicious insider changing intent to bypass controls
- compromised runtime adapter
- confused deputy through stale approvals
- tenant breakout via graph queries
- replay/export misuse

## Major threats

### T1. Intent spoofing
Attacker gửi fake webhook/spec update.

Mitigations:
- signed webhooks
- source trust registry
- actor binding
- idempotency + anti-replay

### T2. Approval confusion
Approval được issue dưới scope cũ nhưng vẫn dùng cho intent mới.

Mitigations:
- approval scope hashing
- policy snapshot binding
- preflight approval revalidation

### T3. Cross-tenant leakage
Graph traversal hoặc replay export lộ dữ liệu tenant khác.

Mitigations:
- tenant isolation in data model
- row-level security
- object store bucket prefix isolation
- export signing and access TTL

### T4. Adapter forgery
Adapter báo checkpoint sai hoặc apply sai rebase plan.

Mitigations:
- signed adapter attestations
- capability registry
- contract tests
- state hash verification

### T5. Audit tampering
Actor xóa hoặc sửa timeline incident.

Mitigations:
- append-only audit log
- WORM/immutable retention tùy tier
- external log sink optional

### T6. Prompt/policy injection via source refs
Spec/ticket chứa nội dung làm lệch diff hoặc rebase classification.

Mitigations:
- source trust tiers
- content sanitization by source type
- low-confidence route to manual review
