//! Graph ingestion primitives
//!
//! Extracted into a dedicated module to keep `lib.rs` bounded.

use crate::GraphService;
use intent_rebase_types::{
    ApprovalIngestRequest, ArtifactIngestRequest, CreateGraphEdgeRequest, CreateGraphNodeRequest,
    EdgeType, IngestorResult, IntentRebaseError, NodeType, SideEffectIngestRequest,
};

impl GraphService {
    /// Ingest an artifact into the graph.
    ///
    /// Creates an Artifact node and wires DependsOn edges to the specified IntentVersion nodes.
    /// This enforces the graph invariant that every artifact traces to at least one intent version.
    ///
    /// # Phase 3 Batch 1 (groundwork): Side Effect Capture Context
    /// When `request.side_effect_context` is provided with sufficient fields, the caller
    /// (typically intent-api) should record a side effect to the compensation ledger
    /// after successful ingest. This enables capture-on-write for artifact-producing
    /// operations that have proper intent/version context.
    ///
    /// **Note:** This method consumes the `side_effect_context` but does NOT automatically
    /// record the side effect. The caller is responsible for checking `request.side_effect_context`
    /// and recording to compensation-service if provided. This separation keeps graph-service
    /// free of compensation-service dependency.
    ///
    /// # Prevalidation
    /// - `depends_on_intent_versions` MUST contain at least one IntentVersion node ID
    /// - All referenced IntentVersion nodes MUST exist, be of type `NodeType::IntentVersion`,
    ///   AND belong to the same tenant_id and workflow_id as the artifact
    pub async fn ingest_artifact(
        &self,
        request: ArtifactIngestRequest,
    ) -> Result<IngestorResult, IntentRebaseError> {
        // Extract side effect context before consuming request
        // Note: The context is consumed but not used by graph-service itself.
        // The caller (e.g., intent-api) should check if context was provided
        // and record the side effect to compensation-service after successful ingest.
        let _side_effect_context = request.side_effect_context.clone();

        // PREVALIDATION: Enforce artifact traceability contract
        if request.depends_on_intent_versions.is_empty() {
            return Err(IntentRebaseError::ArtifactTraceabilityEmpty);
        }

        // Validate all referenced IntentVersion nodes exist, have correct type, and match scope
        for intent_version_id in &request.depends_on_intent_versions {
            // Only map GraphNodeNotFound to InvalidIngestRequest; preserve other errors
            let node = match self.repo.get_node(*intent_version_id).await {
                Ok(n) => n,
                Err(IntentRebaseError::GraphNodeNotFound(_)) => {
                    return Err(IntentRebaseError::InvalidIngestRequest(format!(
                        "IntentVersion node {} does not exist",
                        intent_version_id
                    )));
                }
                Err(e) => return Err(e), // Preserve truthful error classification
            };
            if node.node_type != NodeType::IntentVersion {
                return Err(IntentRebaseError::InvalidIngestRequest(format!(
                    "Node {} is not an IntentVersion (found {:?})",
                    intent_version_id, node.node_type
                )));
            }
            // Validate scope: tenant_id must match
            if node.tenant_id != request.tenant_id {
                return Err(IntentRebaseError::InvalidIngestRequest(format!(
                    "IntentVersion node {} belongs to tenant {} but artifact has tenant {}",
                    intent_version_id, node.tenant_id, request.tenant_id
                )));
            }
            // Validate scope: workflow_id must match
            if node.workflow_id != request.workflow_id {
                return Err(IntentRebaseError::InvalidIngestRequest(format!(
                    "IntentVersion node {} belongs to workflow {} but artifact has workflow {}",
                    intent_version_id, node.workflow_id, request.workflow_id
                )));
            }
        }

        // Create the artifact node (only after prevalidation passes)
        let node_request = CreateGraphNodeRequest {
            tenant_id: request.tenant_id,
            workflow_id: request.workflow_id,
            node_type: NodeType::Artifact,
            external_ref: Some(request.external_ref.clone()),
            label: request.label,
            properties: request.properties,
        };

        let node = self.repo.create_node(node_request).await?;
        let mut edges = Vec::new();

        // Wire DependsOn edges to each IntentVersion
        for intent_version_id in &request.depends_on_intent_versions {
            let edge_request = CreateGraphEdgeRequest {
                tenant_id: request.tenant_id,
                workflow_id: request.workflow_id,
                from_node_id: node.id,
                to_node_id: *intent_version_id,
                edge_type: EdgeType::DependsOn,
                properties: Some(serde_json::json!({
                    "direction": "upstream",
                    "target_type": "IntentVersion"
                })),
            };

            let edge = self.repo.create_edge(edge_request).await?;
            edges.push(edge);
        }

        // Note: If side_effect_context was provided, the caller should record the side effect
        // after successful ingest. The context is available via the consumed request's
        // side_effect_context field. This method does not auto-record to keep graph-service
        // free of compensation-service dependency.

        Ok(IngestorResult { node, edges })
    }

    /// Ingest an approval into the graph.
    ///
    /// Creates an Approval node and optionally wires:
    /// - A GovernedBy edge to the PolicySnapshot that governs this approval
    /// - A ValidatedBy edge to the IntentVersion this approval is associated with
    ///
    /// # Prevalidation
    /// - If `governed_by_policy_snapshot` is provided, the node MUST exist, be of type `NodeType::PolicySnapshot`,
    ///   AND belong to the same tenant_id and workflow_id as the approval
    /// - If `intent_version_id` is provided, the node MUST exist, be of type `NodeType::IntentVersion`,
    ///   AND belong to the same tenant_id and workflow_id as the approval
    pub async fn ingest_approval(
        &self,
        request: ApprovalIngestRequest,
    ) -> Result<IngestorResult, IntentRebaseError> {
        // PREVALIDATION: Validate PolicySnapshot reference if provided
        if let Some(policy_snapshot_id) = request.governed_by_policy_snapshot {
            // Only map GraphNodeNotFound to InvalidIngestRequest; preserve other errors
            let node = match self.repo.get_node(policy_snapshot_id).await {
                Ok(n) => n,
                Err(IntentRebaseError::GraphNodeNotFound(_)) => {
                    return Err(IntentRebaseError::InvalidIngestRequest(format!(
                        "PolicySnapshot node {} does not exist",
                        policy_snapshot_id
                    )));
                }
                Err(e) => return Err(e), // Preserve truthful error classification
            };
            if node.node_type != NodeType::PolicySnapshot {
                return Err(IntentRebaseError::InvalidIngestRequest(format!(
                    "Node {} is not a PolicySnapshot (found {:?})",
                    policy_snapshot_id, node.node_type
                )));
            }
            // Validate scope: tenant_id must match
            if node.tenant_id != request.tenant_id {
                return Err(IntentRebaseError::InvalidIngestRequest(format!(
                    "PolicySnapshot node {} belongs to tenant {} but approval has tenant {}",
                    policy_snapshot_id, node.tenant_id, request.tenant_id
                )));
            }
            // Validate scope: workflow_id must match
            if node.workflow_id != request.workflow_id {
                return Err(IntentRebaseError::InvalidIngestRequest(format!(
                    "PolicySnapshot node {} belongs to workflow {} but approval has workflow {}",
                    policy_snapshot_id, node.workflow_id, request.workflow_id
                )));
            }
        }

        // PREVALIDATION: Validate IntentVersion reference if provided
        if let Some(intent_version_id) = request.intent_version_id {
            // Only map GraphNodeNotFound to InvalidIngestRequest; preserve other errors
            let node = match self.repo.get_node(intent_version_id).await {
                Ok(n) => n,
                Err(IntentRebaseError::GraphNodeNotFound(_)) => {
                    return Err(IntentRebaseError::InvalidIngestRequest(format!(
                        "IntentVersion node {} does not exist",
                        intent_version_id
                    )));
                }
                Err(e) => return Err(e), // Preserve truthful error classification
            };
            if node.node_type != NodeType::IntentVersion {
                return Err(IntentRebaseError::InvalidIngestRequest(format!(
                    "Node {} is not an IntentVersion (found {:?})",
                    intent_version_id, node.node_type
                )));
            }
            // Validate scope: tenant_id must match
            if node.tenant_id != request.tenant_id {
                return Err(IntentRebaseError::InvalidIngestRequest(format!(
                    "IntentVersion node {} belongs to tenant {} but approval has tenant {}",
                    intent_version_id, node.tenant_id, request.tenant_id
                )));
            }
            // Validate scope: workflow_id must match
            if node.workflow_id != request.workflow_id {
                return Err(IntentRebaseError::InvalidIngestRequest(format!(
                    "IntentVersion node {} belongs to workflow {} but approval has workflow {}",
                    intent_version_id, node.workflow_id, request.workflow_id
                )));
            }
        }

        // Create the approval node (only after prevalidation passes)
        let node_request = CreateGraphNodeRequest {
            tenant_id: request.tenant_id,
            workflow_id: request.workflow_id,
            node_type: NodeType::Approval,
            external_ref: Some(request.external_ref.clone()),
            label: request.label,
            properties: request.properties,
        };

        let node = self.repo.create_node(node_request).await?;
        let mut edges = Vec::new();

        // Wire GovernedBy edge to PolicySnapshot if provided
        if let Some(policy_snapshot_id) = request.governed_by_policy_snapshot {
            let edge_request = CreateGraphEdgeRequest {
                tenant_id: request.tenant_id,
                workflow_id: request.workflow_id,
                from_node_id: node.id,
                to_node_id: policy_snapshot_id,
                edge_type: EdgeType::GovernedBy,
                properties: Some(serde_json::json!({
                    "direction": "upstream",
                    "target_type": "PolicySnapshot"
                })),
            };

            let edge = self.repo.create_edge(edge_request).await?;
            edges.push(edge);
        }

        // Wire ValidatedBy edge to IntentVersion if provided
        if let Some(intent_version_id) = request.intent_version_id {
            let edge_request = CreateGraphEdgeRequest {
                tenant_id: request.tenant_id,
                workflow_id: request.workflow_id,
                from_node_id: node.id,
                to_node_id: intent_version_id,
                edge_type: EdgeType::ValidatedBy,
                properties: Some(serde_json::json!({
                    "direction": "upstream",
                    "target_type": "IntentVersion"
                })),
            };

            let edge = self.repo.create_edge(edge_request).await?;
            edges.push(edge);
        }

        Ok(IngestorResult { node, edges })
    }

    /// Ingest a side effect into the graph.
    ///
    /// Creates a SideEffect node and wires appropriate edges to:
    /// - The initiating node that triggered this side effect (Triggers edge, from trigger node to SideEffect)
    /// - The IntentVersion (DerivedFrom edge, from SideEffect to IntentVersion)
    /// - The Approval snapshot if applicable (GeneratedFrom edge, from SideEffect to Approval)
    ///
    /// # Prevalidation
    /// - `triggered_by_task` MUST exist in the graph AND belong to the same tenant_id and workflow_id
    /// - If `derived_from_intent_version` is provided, the node MUST exist, be of type `NodeType::IntentVersion`,
    ///   AND belong to the same tenant_id and workflow_id as the side effect
    /// - If `approval_snapshot_id` is provided, the node MUST exist, be of type `NodeType::Approval`,
    ///   AND belong to the same tenant_id and workflow_id as the side effect
    pub async fn ingest_side_effect(
        &self,
        request: SideEffectIngestRequest,
    ) -> Result<IngestorResult, IntentRebaseError> {
        // PREVALIDATION: Validate triggered_by_task exists and matches scope
        // Only map GraphNodeNotFound to InvalidIngestRequest; preserve other errors
        let triggered_node = match self.repo.get_node(request.triggered_by_task).await {
            Ok(n) => n,
            Err(IntentRebaseError::GraphNodeNotFound(_)) => {
                return Err(IntentRebaseError::InvalidIngestRequest(format!(
                    "Triggering node {} does not exist",
                    request.triggered_by_task
                )));
            }
            Err(e) => return Err(e), // Preserve truthful error classification
        };
        // Validate scope: tenant_id must match
        if triggered_node.tenant_id != request.tenant_id {
            return Err(IntentRebaseError::InvalidIngestRequest(format!(
                "Triggering node {} belongs to tenant {} but side effect has tenant {}",
                request.triggered_by_task, triggered_node.tenant_id, request.tenant_id
            )));
        }
        // Validate scope: workflow_id must match
        if triggered_node.workflow_id != request.workflow_id {
            return Err(IntentRebaseError::InvalidIngestRequest(format!(
                "Triggering node {} belongs to workflow {} but side effect has workflow {}",
                request.triggered_by_task, triggered_node.workflow_id, request.workflow_id
            )));
        }

        // PREVALIDATION: Validate IntentVersion reference if provided
        if let Some(intent_version_id) = request.derived_from_intent_version {
            // Only map GraphNodeNotFound to InvalidIngestRequest; preserve other errors
            let node = match self.repo.get_node(intent_version_id).await {
                Ok(n) => n,
                Err(IntentRebaseError::GraphNodeNotFound(_)) => {
                    return Err(IntentRebaseError::InvalidIngestRequest(format!(
                        "IntentVersion node {} does not exist",
                        intent_version_id
                    )));
                }
                Err(e) => return Err(e), // Preserve truthful error classification
            };
            if node.node_type != NodeType::IntentVersion {
                return Err(IntentRebaseError::InvalidIngestRequest(format!(
                    "Node {} is not an IntentVersion (found {:?})",
                    intent_version_id, node.node_type
                )));
            }
            // Validate scope: tenant_id must match
            if node.tenant_id != request.tenant_id {
                return Err(IntentRebaseError::InvalidIngestRequest(format!(
                    "IntentVersion node {} belongs to tenant {} but side effect has tenant {}",
                    intent_version_id, node.tenant_id, request.tenant_id
                )));
            }
            // Validate scope: workflow_id must match
            if node.workflow_id != request.workflow_id {
                return Err(IntentRebaseError::InvalidIngestRequest(format!(
                    "IntentVersion node {} belongs to workflow {} but side effect has workflow {}",
                    intent_version_id, node.workflow_id, request.workflow_id
                )));
            }
        }

        // PREVALIDATION: Validate Approval reference if provided
        if let Some(approval_snapshot_id) = request.approval_snapshot_id {
            // Only map GraphNodeNotFound to InvalidIngestRequest; preserve other errors
            let node = match self.repo.get_node(approval_snapshot_id).await {
                Ok(n) => n,
                Err(IntentRebaseError::GraphNodeNotFound(_)) => {
                    return Err(IntentRebaseError::InvalidIngestRequest(format!(
                        "Approval node {} does not exist",
                        approval_snapshot_id
                    )));
                }
                Err(e) => return Err(e), // Preserve truthful error classification
            };
            if node.node_type != NodeType::Approval {
                return Err(IntentRebaseError::InvalidIngestRequest(format!(
                    "Node {} is not an Approval (found {:?})",
                    approval_snapshot_id, node.node_type
                )));
            }
            // Validate scope: tenant_id must match
            if node.tenant_id != request.tenant_id {
                return Err(IntentRebaseError::InvalidIngestRequest(format!(
                    "Approval node {} belongs to tenant {} but side effect has tenant {}",
                    approval_snapshot_id, node.tenant_id, request.tenant_id
                )));
            }
            // Validate scope: workflow_id must match
            if node.workflow_id != request.workflow_id {
                return Err(IntentRebaseError::InvalidIngestRequest(format!(
                    "Approval node {} belongs to workflow {} but side effect has workflow {}",
                    approval_snapshot_id, node.workflow_id, request.workflow_id
                )));
            }
        }

        // Create the side effect node (only after prevalidation passes)
        let node_request = CreateGraphNodeRequest {
            tenant_id: request.tenant_id,
            workflow_id: request.workflow_id,
            node_type: NodeType::SideEffect,
            external_ref: Some(request.external_ref.clone()),
            label: request.label,
            properties: request.properties,
        };

        let node = self.repo.create_node(node_request).await?;
        let mut edges = Vec::new();

        // Wire Triggers edge: triggering node -> SideEffect
        let triggers_edge = CreateGraphEdgeRequest {
            tenant_id: request.tenant_id,
            workflow_id: request.workflow_id,
            from_node_id: request.triggered_by_task,
            to_node_id: node.id,
            edge_type: EdgeType::Triggers,
            properties: Some(serde_json::json!({
                "direction": "downstream",
                "target_type": "SideEffect"
            })),
        };
        let triggers_created = self.repo.create_edge(triggers_edge).await?;
        edges.push(triggers_created);

        // Wire DerivedFrom edge: SideEffect -> IntentVersion
        if let Some(intent_version_id) = request.derived_from_intent_version {
            let derived_edge = CreateGraphEdgeRequest {
                tenant_id: request.tenant_id,
                workflow_id: request.workflow_id,
                from_node_id: node.id,
                to_node_id: intent_version_id,
                edge_type: EdgeType::DerivedFrom,
                properties: Some(serde_json::json!({
                    "direction": "upstream",
                    "target_type": "IntentVersion"
                })),
            };

            let derived_created = self.repo.create_edge(derived_edge).await?;
            edges.push(derived_created);
        }

        // Wire GeneratedFrom edge: SideEffect -> Approval (if under approval)
        if let Some(approval_snapshot_id) = request.approval_snapshot_id {
            let generated_edge = CreateGraphEdgeRequest {
                tenant_id: request.tenant_id,
                workflow_id: request.workflow_id,
                from_node_id: node.id,
                to_node_id: approval_snapshot_id,
                edge_type: EdgeType::GeneratedFrom,
                properties: Some(serde_json::json!({
                    "direction": "upstream",
                    "target_type": "Approval"
                })),
            };

            let generated_created = self.repo.create_edge(generated_edge).await?;
            edges.push(generated_created);
        }

        Ok(IngestorResult { node, edges })
    }
}
