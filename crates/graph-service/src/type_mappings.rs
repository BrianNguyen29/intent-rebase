use intent_rebase_types::{EdgeType, ExternalRefType, IntentRebaseError, NodeState, NodeType};

pub(crate) fn node_type_to_string(node_type: &NodeType) -> String {
    match node_type {
        NodeType::Intent => "intent".to_string(),
        NodeType::IntentVersion => "intent_version".to_string(),
        NodeType::Artifact => "artifact".to_string(),
        NodeType::Approval => "approval".to_string(),
        NodeType::PolicySnapshot => "policy_snapshot".to_string(),
        NodeType::SideEffect => "side_effect".to_string(),
        NodeType::Checkpoint => "checkpoint".to_string(),
        NodeType::Workflow => "workflow".to_string(),
        NodeType::Generic => "generic".to_string(),
    }
}

pub(crate) fn node_type_from_string(s: &str) -> Result<NodeType, IntentRebaseError> {
    match s {
        "intent" => Ok(NodeType::Intent),
        "intent_version" => Ok(NodeType::IntentVersion),
        "artifact" => Ok(NodeType::Artifact),
        "approval" => Ok(NodeType::Approval),
        "policy_snapshot" => Ok(NodeType::PolicySnapshot),
        "side_effect" => Ok(NodeType::SideEffect),
        "checkpoint" => Ok(NodeType::Checkpoint),
        "workflow" => Ok(NodeType::Workflow),
        "generic" => Ok(NodeType::Generic),
        _ => Err(IntentRebaseError::Internal(format!(
            "unknown node type: {}",
            s
        ))),
    }
}

pub(crate) fn node_state_to_string(state: &NodeState) -> String {
    match state {
        NodeState::Active => "active".to_string(),
        NodeState::Stale => "stale".to_string(),
        NodeState::Invalid => "invalid".to_string(),
        NodeState::Archived => "archived".to_string(),
    }
}

pub(crate) fn node_state_from_string(s: &str) -> Result<NodeState, IntentRebaseError> {
    match s {
        "active" => Ok(NodeState::Active),
        "stale" => Ok(NodeState::Stale),
        "invalid" => Ok(NodeState::Invalid),
        "archived" => Ok(NodeState::Archived),
        _ => Err(IntentRebaseError::Internal(format!(
            "unknown node state: {}",
            s
        ))),
    }
}

pub(crate) fn edge_type_to_string(edge_type: &EdgeType) -> String {
    match edge_type {
        EdgeType::DependsOn => "depends_on".to_string(),
        EdgeType::Produces => "produces".to_string(),
        EdgeType::Approves => "approves".to_string(),
        EdgeType::Triggers => "triggers".to_string(),
        EdgeType::Defines => "defines".to_string(),
        EdgeType::GeneratedFrom => "generated_from".to_string(),
        EdgeType::ValidatedBy => "validated_by".to_string(),
        EdgeType::GovernedBy => "governed_by".to_string(),
        EdgeType::DerivedFrom => "derived_from".to_string(),
        EdgeType::StoredIn => "stored_in".to_string(),
        EdgeType::Supersedes => "supersedes".to_string(),
        EdgeType::Blocks => "blocks".to_string(),
        EdgeType::Compensates => "compensates".to_string(),
    }
}

pub(crate) fn edge_type_from_string(s: &str) -> Result<EdgeType, IntentRebaseError> {
    match s {
        "depends_on" => Ok(EdgeType::DependsOn),
        "produces" => Ok(EdgeType::Produces),
        "approves" => Ok(EdgeType::Approves),
        "triggers" => Ok(EdgeType::Triggers),
        "defines" => Ok(EdgeType::Defines),
        "generated_from" => Ok(EdgeType::GeneratedFrom),
        "validated_by" => Ok(EdgeType::ValidatedBy),
        "governed_by" => Ok(EdgeType::GovernedBy),
        "derived_from" => Ok(EdgeType::DerivedFrom),
        "stored_in" => Ok(EdgeType::StoredIn),
        "supersedes" => Ok(EdgeType::Supersedes),
        "blocks" => Ok(EdgeType::Blocks),
        "compensates" => Ok(EdgeType::Compensates),
        _ => Err(IntentRebaseError::Internal(format!(
            "unknown edge type: {}",
            s
        ))),
    }
}

pub(crate) fn external_ref_type_to_string(ref_type: &ExternalRefType) -> String {
    match ref_type {
        ExternalRefType::Intent => "intent".to_string(),
        ExternalRefType::IntentVersion => "intent_version".to_string(),
        ExternalRefType::Artifact => "artifact".to_string(),
        ExternalRefType::Approval => "approval".to_string(),
        ExternalRefType::PolicySnapshot => "policy_snapshot".to_string(),
        ExternalRefType::SideEffect => "side_effect".to_string(),
        ExternalRefType::Checkpoint => "checkpoint".to_string(),
    }
}

pub(crate) fn external_ref_type_from_string(s: &str) -> Result<ExternalRefType, IntentRebaseError> {
    match s {
        "intent" => Ok(ExternalRefType::Intent),
        "intent_version" => Ok(ExternalRefType::IntentVersion),
        "artifact" => Ok(ExternalRefType::Artifact),
        "approval" => Ok(ExternalRefType::Approval),
        "policy_snapshot" => Ok(ExternalRefType::PolicySnapshot),
        "side_effect" => Ok(ExternalRefType::SideEffect),
        "checkpoint" => Ok(ExternalRefType::Checkpoint),
        _ => Err(IntentRebaseError::Internal(format!(
            "unknown external ref type: {}",
            s
        ))),
    }
}
