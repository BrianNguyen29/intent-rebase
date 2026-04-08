-- Migration: 005_create_graph_edges.sql
-- Description: Create graph_edges table for Phase 1 graph storage baseline
-- Created: Phase 1 PR #9 - graph storage baseline
-- Storage: Postgres with relational edge tables (not graph DB)
-- See: docs/03-spec/03-dependency-graph.md (storage strategy)

CREATE TABLE IF NOT EXISTS graph_edges (
    edge_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    workflow_id UUID NOT NULL,
    
    -- Edge endpoints (nodes must exist)
    from_node_id UUID NOT NULL REFERENCES graph_nodes(node_id) ON DELETE CASCADE,
    to_node_id UUID NOT NULL REFERENCES graph_nodes(node_id) ON DELETE CASCADE,
    
    -- Edge type label
    edge_type VARCHAR(30) NOT NULL CHECK (edge_type IN (
        'depends_on', 'produces', 'approves', 'triggers', 'defines',
        'generated_from', 'validated_by', 'governed_by', 'derived_from',
        'stored_in', 'supersedes', 'blocks', 'compensates'
    )),
    
    -- Flexible properties as JSONB for edge metadata
    properties JSONB NOT NULL DEFAULT '{}',
    
    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    
    -- Prevent duplicate edges (same from/to/type combination)
    CONSTRAINT unique_edge UNIQUE (tenant_id, from_node_id, to_node_id, edge_type)
);

-- Indexes for common query patterns
CREATE INDEX IF NOT EXISTS idx_graph_edges_tenant_id ON graph_edges(tenant_id);
CREATE INDEX IF NOT EXISTS idx_graph_edges_workflow_id ON graph_edges(workflow_id);
CREATE INDEX IF NOT EXISTS idx_graph_edges_from_node ON graph_edges(from_node_id);
CREATE INDEX IF NOT EXISTS idx_graph_edges_to_node ON graph_edges(to_node_id);
CREATE INDEX IF NOT EXISTS idx_graph_edges_edge_type ON graph_edges(edge_type);
CREATE INDEX IF NOT EXISTS idx_graph_edges_created_at ON graph_edges(created_at DESC);

-- Composite indexes for common filter combinations
CREATE INDEX IF NOT EXISTS idx_graph_edges_tenant_workflow ON graph_edges(tenant_id, workflow_id);
CREATE INDEX IF NOT EXISTS idx_graph_edges_from_tenant ON graph_edges(from_node_id, tenant_id);
CREATE INDEX IF NOT EXISTS idx_graph_edges_to_tenant ON graph_edges(to_node_id, tenant_id);

-- Post-migration validation notes:
-- 1. Verify tables exist: SELECT COUNT(*) FROM graph_edges; SELECT COUNT(*) FROM meta_edge_types;
-- 2. Verify indexes exist: SELECT indexname FROM pg_indexes WHERE tablename IN ('graph_edges', 'meta_edge_types');
-- 3. Verify CHECK constraints: SELECT conname FROM pg_constraint WHERE conrelid = 'graph_edges'::regclass AND contype = 'c';
-- 4. Verify UNIQUE constraint: SELECT conname FROM pg_constraint WHERE conrelid = 'graph_edges'::regclass AND contype = 'u';
-- 5. Verify foreign keys: SELECT conname FROM pg_constraint WHERE conrelid = 'graph_edges'::regclass AND contype = 'f';
-- 6. Verify trigger exists: SELECT trgname FROM pg_trigger WHERE tgrelid = 'graph_edges'::regclass;

-- Rollback:
-- DROP TRIGGER IF EXISTS graph_edges_validate_tenant_workflow ON graph_edges;
-- DROP FUNCTION IF EXISTS trg_validate_edge_node_tenant_workflow_match();
-- DROP TABLE IF EXISTS graph_edges;
-- DROP TABLE IF EXISTS meta_edge_types;
-- (Order matters: graph_edges depends on graph_nodes via FK, and trigger depends on function)

-- Comments
COMMENT ON TABLE graph_edges IS 'Phase 1: Graph edges for dependency tracking - Postgres-backed relational storage';
COMMENT ON COLUMN graph_edges.from_node_id IS 'Source node of the edge';
COMMENT ON COLUMN graph_edges.to_node_id IS 'Target node of the edge';
COMMENT ON COLUMN graph_edges.edge_type IS 'Semantic label for the relationship';
COMMENT ON COLUMN graph_edges.properties IS 'Flexible JSONB properties for edge-specific data';

-- Trigger function: Enforce tenant/workflow integrity for edges
-- An edge can only connect nodes that belong to the same tenant/workflow as the edge itself.
-- This prevents silent cross-tenant or cross-workflow edge creation that would bypass service-level validation.
CREATE OR REPLACE FUNCTION trg_validate_edge_node_tenant_workflow_match()
RETURNS TRIGGER AS $$
DECLARE
    from_tenant UUID;
    from_workflow UUID;
    to_tenant UUID;
    to_workflow UUID;
BEGIN
    -- Look up the tenant_id and workflow_id of the from_node
    SELECT tenant_id, workflow_id INTO from_tenant, from_workflow
    FROM graph_nodes
    WHERE node_id = NEW.from_node_id;

    -- Look up the tenant_id and workflow_id of the to_node
    SELECT tenant_id, workflow_id INTO to_tenant, to_workflow
    FROM graph_nodes
    WHERE node_id = NEW.to_node_id;

    -- Validate from_node matches edge's tenant/workflow
    IF from_tenant IS NULL OR from_workflow IS NULL THEN
        RAISE EXCEPTION 'from_node_id % does not exist in graph_nodes', NEW.from_node_id;
    END IF;

    IF from_tenant != NEW.tenant_id THEN
        RAISE EXCEPTION 'Edge tenant_id % does not match from_node_id % tenant_id %',
            NEW.tenant_id, NEW.from_node_id, from_tenant;
    END IF;

    IF from_workflow != NEW.workflow_id THEN
        RAISE EXCEPTION 'Edge workflow_id % does not match from_node_id % workflow_id %',
            NEW.workflow_id, NEW.from_node_id, from_workflow;
    END IF;

    -- Validate to_node matches edge's tenant/workflow
    IF to_tenant IS NULL OR to_workflow IS NULL THEN
        RAISE EXCEPTION 'to_node_id % does not exist in graph_nodes', NEW.to_node_id;
    END IF;

    IF to_tenant != NEW.tenant_id THEN
        RAISE EXCEPTION 'Edge tenant_id % does not match to_node_id % tenant_id %',
            NEW.tenant_id, NEW.to_node_id, to_tenant;
    END IF;

    IF to_workflow != NEW.workflow_id THEN
        RAISE EXCEPTION 'Edge workflow_id % does not match to_node_id % workflow_id %',
            NEW.workflow_id, NEW.to_node_id, to_workflow;
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Attach the trigger to graph_edges (fires BEFORE INSERT OR UPDATE of edge rows)
CREATE TRIGGER graph_edges_validate_tenant_workflow
    BEFORE INSERT OR UPDATE ON graph_edges
    FOR EACH ROW
    EXECUTE FUNCTION trg_validate_edge_node_tenant_workflow_match();

-- Add foreign key constraints for edge_type reference
-- (edge types are constrained by CHECK but we also maintain a reference table for documentation)
CREATE TABLE IF NOT EXISTS meta_edge_types (
    edge_type VARCHAR(30) PRIMARY KEY,
    description TEXT
);

INSERT INTO meta_edge_types (edge_type, description) VALUES
    ('depends_on', 'Source depends on target'),
    ('produces', 'Source produces/creates target'),
    ('approves', 'Source approves target'),
    ('triggers', 'Source triggers target'),
    ('defines', 'Source defines target'),
    ('generated_from', 'Target was generated from source'),
    ('validated_by', 'Source validated by target'),
    ('governed_by', 'Source governed by target'),
    ('derived_from', 'Source derived from target'),
    ('stored_in', 'Source stored in target'),
    ('supersedes', 'Source supersedes target'),
    ('blocks', 'Source blocks target'),
    ('compensates', 'Source compensates for target')
ON CONFLICT (edge_type) DO NOTHING;
