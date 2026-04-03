# Intent Rebase Engine

Phase 1 first slice implementation — Intent Registry. A control layer that manages intent versioning, semantic diff, dependency graphs, and rebase operations for agent runtimes.

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
├── infrastructure/
│   ├── local/
│   │   └── docker-compose.yml # Local dev environment
│   └── migrations/            # SQL migration files
│
└── docs/
    └── 04-api/
        └── openapi.yaml       # OpenAPI 3.0 specification
```

## Crates

| Crate | Purpose | Phase | Status |
|-------|---------|-------|--------|
| `intent-rebase-types` | Shared types: Intent, Artifact, GraphNode, AuditEvent, error types | P0 | ✅ Complete |
| `intent-service` | Intent lifecycle (create, read, update, version) | P1 | ✅ First slice |
| `rebase-engine` | Semantic diff, rebase plan generation | P1 | 🔜 Planned |
| `graph-service` | Dependency graph CRUD and propagation | P1 | 🔜 Planned |

## Implementation Status

### Phase 1 First Slice — Intent Registry (Current)
**Implemented:**
- Intent domain types matching `docs/03-spec/01-intent-model.md`
- Intent service with create, version management
- In-memory repository (SQL repository + migrations as baseline)
- Migration files for `intents`, `intent_versions`, `intent_clauses` tables
- OpenAPI 3.0 skeleton at `docs/04-api/openapi.yaml`

**Planned for Phase 1 full:**
- SQL-backed repository integration
- Full optimistic concurrency controls
- HTTP server framework integration

### Phase 2+ — Diff, Rebase, Graph
- Semantic diff engine
- Rebase planner
- Impact graph propagation
- Runtime adapter integration

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

# Apply database migrations (when ready)
# psql $DATABASE_URL -f infrastructure/migrations/001_create_intents.sql
# psql $DATABASE_URL -f infrastructure/migrations/002_create_intent_versions.sql
# psql $DATABASE_URL -f infrastructure/migrations/003_create_intent_clauses.sql
```

## API Endpoints (Phase 1 Implemented)

| Method | Endpoint | Description |
|--------|----------|-------------|
| POST | `/v1/intents` | Create a new intent |
| GET | `/v1/intents/{intent_id}` | Get intent head (current version) |
| POST | `/v1/intents/{intent_id}/versions` | Create a new version |
| GET | `/v1/intents/{intent_id}/versions` | List all versions |
| GET | `/v1/intents/{intent_id}/versions/{version_number}` | Get specific version |

Full OpenAPI spec: `docs/04-api/openapi.yaml`

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
