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
│   ├── intent-api/            # HTTP transport layer (axum)
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
| `intent-service` | Intent lifecycle (create, read, update, version) | P1 | ✅ Complete |
| `intent-api` | HTTP transport layer with axum | P1 | ✅ Complete |
| `rebase-engine` | Structured diff core (scope, constraints, acceptance, authority); rebase planning deferred to Phase 2 | P1 | 🟡 Partial |
| `graph-service` | Dependency graph CRUD and propagation | P1 | 🔜 Planned |

## Implementation Status

### Phase 1 First Slice — Intent Registry (Current)
**Implemented:**
- Intent domain types matching `docs/03-spec/01-intent-model.md`
- Intent service with create, version management
- In-memory repository for testing
- SQL-backed repository (`SqlxIntentRepository`) with PostgreSQL/sqlx
- Optimistic concurrency control (OCC) for version creation via `create_version_with_occ`
- HTTP transport layer using axum with manual route binding (CORS only — no tracing middleware)
- Migration files for `intents`, `intent_versions`, `intent_clauses` tables
- OpenAPI 3.0 spec at `docs/04-api/openapi.yaml` (manually wired to handlers)
- Routes mount directly; intended to be served under `/v1` prefix in production

**Deferred to Phase 2+:**
- Full diff/rebase/graph operations
- Full authentication/authorization
- DB integration tests in CI (SQL repository is implemented but live-DB tests are skipped)

### Phase 1 Second Slice — Structured Diff Core (PR #5)
**Implemented:**
- `rebase-engine` structured diff for 4 sections: scope, constraints, acceptance_criteria, authority
- Deterministic output ordering (sorted by clause_id/section)
- Conservative matching rules: prefer clause_id matching; fallback to add/remove when identity ambiguous
- Typed diff output via `IntentVersionDiff` and related types
- Synchronous (`compute_diff_sync`) and async (`RebaseEngine::compute_diff`) APIs

### Phase 1 Third Slice — Diff Risk Rules (PR #6)
**Implemented:**
- Engine-local severity assignment (low/medium/high/critical) based on change type and affected section
- Deterministic confidence scoring (0.0-1.0) based on clause_id matching quality
- Manual-review triggers for critical severity, low confidence, policy changes, multiple high-severity changes
- Typed risk output via `DiffRiskAnalysis`, `Severity`, `RiskConfig` types
- APIs: `analyze_diff_risk()`, `compute_diff_with_risk_sync()`, `RebaseEngine::compute_diff_with_risk()`
- Tests covering all severity levels, confidence thresholds, and manual-review trigger conditions

**NOT implemented in this slice (deferred to future PRs):**
- HTTP endpoints for diff (OpenAPI spec not updated)
- Graph model and rebase planner integration

**Notes:**
- `intent-api` crate exposes axum router; `build_router()` returns Router that mounts directly
- No tracing middleware (only CORS layer enabled in this PR)
- OCC headers (`X-Expected-Version`, `X-Expected-Row-Version`) are validated at the API boundary:
  malformed headers (non-integer values) return 400 Bad Request instead of being silently ignored
- SQL deserialization returns 500 SerializationError on data corruption; no silent payload fabrication

### Phase 2+ — Rebase, Graph, Extended Diff
- Rebase planner (graph-based)
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

Routes are mounted directly (e.g., `POST /intents`) and intended to be served under `/v1` prefix in production deployments.

| Method | Endpoint | Description |
|--------|----------|-------------|
| POST | `/v1/intents` | Create a new intent |
| GET | `/v1/intents/{intent_id}` | Get intent head (current version) |
| POST | `/v1/intents/{intent_id}/versions` | Create a new version (supports OCC via `X-Expected-Version` / `X-Expected-Row-Version` headers) |
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
