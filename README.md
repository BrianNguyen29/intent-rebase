# Intent Rebase Engine

Phase 0 bootstrap workspace for the Intent Rebase Engine — a control layer that manages intent versioning, semantic diff, dependency graphs, and rebase operations for agent runtimes.

## Workspace Structure

```
├── Cargo.toml                 # Root workspace manifest
├── rust-toolchain.toml        # Rust stable toolchain (1.75+)
│
├── crates/
│   ├── intent-rebase-types/   # Shared domain types, traits, errors
│   ├── intent-service/        # Intent CRUD and versioning service
│   ├── rebase-engine/         # Semantic diff and rebase planning
│   └── graph-service/         # Dependency graph management
│
└── infrastructure/
    └── local/
        └── docker-compose.yml # Local dev environment
```

## Crates

| Crate | Purpose | Phase |
|-------|---------|-------|
| `intent-rebase-types` | Shared types: Intent, Artifact, GraphNode, AuditEvent, error types | P0 |
| `intent-service` | Intent lifecycle (create, read, update, version) | P1 |
| `rebase-engine` | Semantic diff, rebase plan generation | P1 |
| `graph-service` | Dependency graph CRUD and propagation | P1 |

## Status

**Phase 0 — Bootstrap**: Workspace scaffolding, CI baseline, local infra baseline, ADR acceptance.

## Quick Start

```bash
# Start local infra
docker compose -f infrastructure/local/docker-compose.yml up -d

# Check workspace compiles
cargo check --workspace

# Run tests
cargo test --workspace

# Format check
cargo fmt --all -- --check
```

## ADRs

All ADRs are in `docs/13-adrs/` and reflect accepted/deferred status as of Phase 0.

## Local Environment Variables

Copy `.env.example` to `.env` and fill in values. Required variables:

```env
DATABASE_URL=postgres://intent_rebase:intent_rebase_dev@localhost:5432/intent_rebase
NATS_URL=nats://localhost:4222
AWS_ACCESS_KEY_ID=minioadmin
AWS_SECRET_ACCESS_KEY=minioadmin
S3_ENDPOINT=http://localhost:9000
S3_REGION=us-east-1
S3_BUCKET=intent-rebase-artifacts
```
