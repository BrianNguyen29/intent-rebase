# Backend Service Boundaries

## Service map

### intent-api
- public/API gateway-facing
- create/read/update intent versions
- authn/authz enforcement
- idempotency

### diff-service
- semantic diff computation
- hybrid rule + model-assisted classification

### graph-service
- node/edge upserts
- impact traversal
- graph snapshots

### rebase-service
- decision engine
- repair plan generation
- checkpoint selection

### policy-service
- approval revalidation
- policy snapshot resolution
- risk gating

### artifact-service
- artifact metadata
- storage URI management
- quarantine / status transitions

### side-effect-service
- record side effects
- compensation planning/execution

### adapter-service
- runtime-specific orchestration bridge
- Temporal/LangGraph/custom adapter plugins

### audit-service
- append-only audit events
- replay exports
- forensic timeline

### notification-service
- webhooks, email, chatops, internal events

## Boundary rules
- services giao tiếp qua gRPC hoặc async events nội bộ
- cross-service writes nên đi qua explicit APIs hoặc transactional outbox
- không query DB chéo trực tiếp trừ analytics/read models
