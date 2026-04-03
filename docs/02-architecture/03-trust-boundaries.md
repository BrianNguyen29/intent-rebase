# Trust Boundaries

## Boundary A: External Sources -> Intent Ingestion
Rủi ro:
- malformed payload
- forged actor identity
- malicious spec injection
- duplicated events

Controls:
- signed webhook verification
- schema validation
- idempotency keys
- source trust tiers
- source-specific sanitization

## Boundary B: Control Plane -> Runtime Adapters
Rủi ro:
- adapter chạy sai rebase plan
- inconsistent checkpoint mapping
- runtime không hỗ trợ pause/resume chuẩn

Controls:
- adapter capability registry
- contract tests
- fallback modes
- explicit support matrix

## Boundary C: Control Plane -> Side Effects
Rủi ro:
- action thực thi theo intent cũ
- approval stale nhưng vẫn được dùng
- compensation chạy sai scope

Controls:
- action preflight with current intent head
- approval snapshot validation
- action tokens with intent_version binding

## Boundary D: Tenant Isolation
Rủi ro:
- graph traversal cross-tenant
- leaked artifacts
- replay logs lẫn tenant

Controls:
- row-level security
- tenant-scoped encryption keys
- per-tenant topic partitioning
- signed tenant context in every request

## Boundary E: Operator Console
Rủi ro:
- unauthorized force override
- hidden diff causing operator mistakes

Controls:
- least privilege RBAC/ABAC
- four-eyes approval for risky overrides
- immutable operator actions log
