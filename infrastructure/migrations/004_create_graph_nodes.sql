-- Migration: 004_create_graph_nodes.sql
-- Description: Create graph_nodes table for Phase 1 graph storage baseline
-- Created: Phase 1 PR #9 - graph storage baseline
-- Storage: Postgres with relational edge tables (not graph DB)
-- See: docs/03-spec/03-dependency-graph.md (storage strategy)

-- Reference table for external ref types (MUST be created BEFORE graph_nodes due to FK)
CREATE TABLE IF NOT EXISTS meta_ref_types (
    ref_type VARCHAR(30) PRIMARY KEY,
    description TEXT
);

-- Populate ref types
INSERT INTO meta_ref_types (ref_type, description) VALUES
    ('intent', 'References an intent document'),
    ('intent_version', 'References a specific version of an intent'),
    ('artifact', 'References an artifact/output'),
    ('approval', 'References an approval record'),
    ('policy_snapshot', 'References a policy snapshot'),
    ('side_effect', 'References a side effect record'),
    ('checkpoint', 'References a runtime checkpoint')
ON CONFLICT (ref_type) DO NOTHING;

CREATE TABLE IF NOT EXISTS graph_nodes (
    node_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    workflow_id UUID NOT NULL,
    node_type VARCHAR(30) NOT NULL CHECK (node_type IN (
        'intent', 'intent_version', 'artifact', 'approval',
        'policy_snapshot', 'side_effect', 'checkpoint', 'workflow', 'generic'
    )),
    
    -- External reference (what this node represents in external systems)
    external_ref_type VARCHAR(30) REFERENCES meta_ref_types(ref_type), -- nullable
    external_ref_id UUID,
    
    -- Human-readable label for the node
    label VARCHAR(255) NOT NULL,
    
    -- Current state of this node in the graph
    state VARCHAR(20) NOT NULL DEFAULT 'active' CHECK (state IN ('active', 'stale', 'invalid', 'archived')),
    
    -- Flexible properties as JSONB for extensibility
    properties JSONB NOT NULL DEFAULT '{}',
    
    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    
    -- Unique constraint on external ref to prevent duplicate nodes for same entity
    CONSTRAINT unique_external_ref UNIQUE (tenant_id, external_ref_type, external_ref_id)
);

-- Indexes for common query patterns
CREATE INDEX idx_graph_nodes_tenant_id ON graph_nodes(tenant_id);
CREATE INDEX idx_graph_nodes_workflow_id ON graph_nodes(workflow_id);
CREATE INDEX idx_graph_nodes_node_type ON graph_nodes(node_type);
CREATE INDEX idx_graph_nodes_state ON graph_nodes(state);
CREATE INDEX idx_graph_nodes_external_ref ON graph_nodes(external_ref_type, external_ref_id);
CREATE INDEX idx_graph_nodes_created_at ON graph_nodes(created_at DESC);

-- Composite index for common filter combinations
CREATE INDEX idx_graph_nodes_tenant_workflow ON graph_nodes(tenant_id, workflow_id);
CREATE INDEX idx_graph_nodes_tenant_type ON graph_nodes(tenant_id, node_type);

-- Comments
COMMENT ON TABLE graph_nodes IS 'Phase 1: Graph nodes for dependency tracking - Postgres-backed relational storage';
COMMENT ON COLUMN graph_nodes.external_ref_type IS 'Type of external entity this node represents';
COMMENT ON COLUMN graph_nodes.external_ref_id IS 'ID of the external entity';
COMMENT ON COLUMN graph_nodes.state IS 'Node state: active/stale/invalid/archived';
COMMENT ON COLUMN graph_nodes.properties IS 'Flexible JSONB properties for node-specific data';
