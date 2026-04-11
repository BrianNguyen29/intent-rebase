# Backend Service Boundaries

> **Target-state topology vs. current implementation:** This document describes the target-state service topology as designed. The **current implementation inventory** is listed in the [Implementation column](#current-implementation-inventory) below. Not all target-state services are yet implemented as isolated crates.

## Current Implementation Inventory

| Implemented Crate | Role |
|--------------------|------|
| `intent-api` | Public API gateway; intent CRUD, authn/authz, idempotency |
| `intent-service` | Intent processing service (core business logic) |
| `graph-service` | Dependency graph service; node/edge ops, impact traversal |
| `rebase-engine` | Rebase decision engine; diff, repair plan, checkpoint selection |
| `rebase-orchestrator` | Rebase workflow orchestration and control-plane coordination |
| `runtime-adapter` | Runtime-specific orchestration bridge; Temporal/custom adapter plugins |
| `compensation-service` | Side effect ledger; compensation action management, orchestration runtime |
| `forensic-service` | Forensic bundle replay and export |
| `intent-cli` | CLI client for orchestration run operations |
| `intent-rebase-types` | Shared core type definitions |

## Target-State Topology (not yet fully implemented)

### intent-api
- public/API gateway-facing
- create/read/update intent versions
- authn/authz enforcement
- idempotency

### diff-service *(planned)*
- semantic diff computation
- hybrid rule + model-assisted classification

### graph-service ✓ *(implemented)*
- node/edge upserts
- impact traversal
- graph snapshots

### rebase-service *(planned)*
- decision engine
- repair plan generation
- checkpoint selection

### policy-service *(planned)*
- approval revalidation
- policy snapshot resolution
- risk gating

### artifact-service *(planned)*
- artifact metadata
- storage URI management
- quarantine / status transitions

### compensation-service ✓ *(implemented)*
- record side effects
- compensation planning/execution
- orchestration runtime

### adapter-service *(planned)*
- runtime-specific orchestration bridge
- Temporal/LangGraph/custom adapter plugins

### audit-service *(planned)*
- append-only audit events
- replay exports
- forensic timeline

### notification-service *(planned)*
- webhooks, email, chatops, internal events

## Boundary rules
- services giao tiếp qua gRPC hoặc async events nội bộ
- cross-service writes nên đi qua explicit APIs hoặc transactional outbox
- không query DB chéo trực tiếp trừ analytics/read models
