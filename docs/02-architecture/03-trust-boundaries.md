# Trust Boundaries

## Boundary A: External Sources -> Intent Ingestion
Risks:
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
Risks:
- adapter executes wrong rebase plan
- inconsistent checkpoint mapping
- runtime does not support standard pause/resume

Controls:
- adapter capability registry
- contract tests
- fallback modes
- explicit support matrix

## Boundary C: Control Plane -> Side Effects
Risks:
- action executes according to old intent
- stale approval still being used
- compensation runs in wrong scope

Controls:
- action preflight with current intent head
- approval snapshot validation
- action tokens with intent_version binding

## Boundary D: Tenant Isolation
Risks:
- graph traversal cross-tenant
- leaked artifacts
- replay logs mixed across tenants

Controls:
- row-level security
- tenant-scoped encryption keys
- per-tenant topic partitioning
- signed tenant context in every request

## Boundary E: Operator Console
Risks:
- unauthorized force override
- hidden diff causing operator mistakes

Controls:
- least privilege RBAC/ABAC
- four-eyes approval for risky overrides
- immutable operator actions log
