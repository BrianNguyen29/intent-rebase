//! Intent domain types
//!
//! Phase 1: Expanded to match intent-model specification.

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use validator::Validate;

/// Source reference for tracking external documents
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SourceRef {
    #[serde(rename = "type")]
    pub ref_type: String,
    pub id: String,
}

/// Change channel indicating how a version was created
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ChangeChannel {
    UserEdit,
    Webhook,
    PolicyUpdate,
    SystemNormalization,
}

/// Status of an intent document
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum IntentStatus {
    Active,
    Archived,
    Superseded,
}

/// Status of an intent version
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum VersionStatus {
    Draft,
    Active,
    Rejected,
    Superseded,
}

/// Clause type for intent constraints
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ClauseType {
    Functional,
    NonFunctional,
    Policy,
    Budget,
    Time,
}

/// Operator for constraint evaluation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConstraintOperator {
    Eq,
    Neq,
    Lt,
    Lte,
    Gt,
    Gte,
    Contains,
    NotContains,
    Regex,
    Custom,
}

/// Priority of a clause
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ClausePriority {
    Must,
    Should,
    Could,
}

/// Risk tier for an intent
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RiskTier {
    Low,
    Medium,
    High,
    Critical,
}

/// Urgency level for an intent
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Urgency {
    Low,
    Medium,
    High,
    Critical,
}

/// Tradeoff dimension for preferences
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TradeoffDimension {
    Speed,
    Cost,
    Quality,
    Risk,
    Compatibility,
    Latency,
}

/// Tradeoff preference
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TradeoffPreference {
    Prioritize,
    Balance,
    Minimize,
    Maximize,
}

/// Document reference for specs, tickets, repos, policies
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DocRef {
    #[serde(rename = "type")]
    pub ref_type: String,
    pub id: String,
    pub uri: Option<String>,
}

/// Actor reference (who performed an action)
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Validate)]
pub struct ActorRef {
    pub actor_type: String,
    pub actor_id: String,
}

/// Objective section of intent payload
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Validate)]
pub struct IntentObjective {
    #[validate(length(min = 1, max = 500))]
    pub summary: String,
    #[validate(length(min = 1, max = 2000))]
    pub success_statement: String,
    #[validate(length(min = 1, max = 100))]
    pub domain: String,
}

/// Scope section defining in/out of scope items
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Validate)]
pub struct IntentScope {
    #[serde(rename = "in_scope")]
    pub in_scope: Vec<String>,
    #[serde(rename = "out_of_scope")]
    pub out_of_scope: Vec<String>,
}

/// A single constraint within the constraints section
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct Constraint {
    pub clause_id: Option<Uuid>,
    #[serde(rename = "type")]
    pub constraint_type: ClauseType,
    #[validate(length(min = 1, max = 200))]
    pub key: String,
    pub operator: ConstraintOperator,
    pub value: serde_json::Value,
    pub rationale: Option<String>,
    pub priority: ClausePriority,
}

/// Constraints section grouping all constraint types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentConstraints {
    pub functional: Vec<Constraint>,
    pub non_functional: Vec<Constraint>,
    pub policy: Vec<Constraint>,
    pub budget: Vec<Constraint>,
    pub time: Vec<Constraint>,
}

/// Acceptance criterion
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct AcceptanceCriterion {
    pub clause_id: Option<Uuid>,
    #[validate(length(min = 1, max = 500))]
    pub description: String,
    pub priority: ClausePriority,
}

/// Acceptance criteria section
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcceptanceCriteria {
    pub required: Vec<AcceptanceCriterion>,
    pub optional: Vec<AcceptanceCriterion>,
}

/// Action reference
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ActionRef {
    pub action: String,
    pub target: Option<String>,
}

/// Approval rule reference
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ApprovalRuleRef {
    pub rule_id: String,
    pub description: String,
}

/// Authority section defining allowed/forbidden actions
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct IntentAuthority {
    #[serde(rename = "allowed_actions")]
    pub allowed_actions: Vec<ActionRef>,
    #[serde(rename = "forbidden_actions")]
    pub forbidden_actions: Vec<ActionRef>,
    #[serde(rename = "approval_requirements")]
    pub approval_requirements: Vec<ApprovalRuleRef>,
}

/// Tradeoff preference entry
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Tradeoff {
    pub dimension: TradeoffDimension,
    pub preference: TradeoffPreference,
}

/// Preferences section
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct IntentPreferences {
    pub tradeoffs: Vec<Tradeoff>,
}

/// References section linking to external documents
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct IntentReferences {
    pub specs: Vec<DocRef>,
    pub tickets: Vec<DocRef>,
    pub repos: Vec<DocRef>,
    pub policies: Vec<DocRef>,
}

/// Assumptions section with explicit assumptions
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Validate)]
pub struct IntentAssumptions {
    pub explicit: Vec<String>,
}

/// Intent metadata for risk/urgency/confidence
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Validate)]
pub struct IntentMetadataV1 {
    #[serde(rename = "risk_tier")]
    pub risk_tier: RiskTier,
    pub urgency: Urgency,
    #[validate(range(min = 0.0, max = 1.0))]
    pub confidence: f64,
}

/// Full intent payload structure
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct IntentPayload {
    #[validate(nested)]
    pub objective: IntentObjective,
    pub scope: IntentScope,
    pub constraints: IntentConstraints,
    #[serde(rename = "acceptance_criteria")]
    pub acceptance_criteria: AcceptanceCriteria,
    pub authority: IntentAuthority,
    pub preferences: IntentPreferences,
    pub references: IntentReferences,
    pub assumptions: IntentAssumptions,
    #[validate(nested)]
    pub metadata: IntentMetadataV1,
}

/// An intent document managed by IRE
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Intent {
    pub id: Uuid,
    #[serde(rename = "tenant_id")]
    pub tenant_id: Uuid,
    #[serde(rename = "workflow_id")]
    pub workflow_id: Uuid,
    #[serde(rename = "current_version")]
    pub current_version: i32,
    pub status: IntentStatus,
    #[serde(rename = "created_at")]
    pub created_at: DateTime<Utc>,
    #[serde(rename = "created_by")]
    pub created_by: ActorRef,
    #[serde(rename = "source_refs")]
    pub source_refs: Vec<SourceRef>,
    pub tags: Vec<String>,
}

/// An intent version representing a specific snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentVersion {
    pub id: Uuid,
    #[serde(rename = "intent_id")]
    pub intent_id: Uuid,
    #[serde(rename = "version_number")]
    pub version_number: i32,
    #[serde(rename = "parent_version_id")]
    pub parent_version_id: Option<Uuid>,
    #[serde(rename = "created_at")]
    pub created_at: DateTime<Utc>,
    #[serde(rename = "created_by")]
    pub created_by: ActorRef,
    #[serde(rename = "change_reason")]
    pub change_reason: String,
    #[serde(rename = "change_channel")]
    pub change_channel: ChangeChannel,
    pub status: VersionStatus,
    pub hash: String,
    pub payload: IntentPayload,
}

/// Intent clause for fine-grained tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentClause {
    pub id: Uuid,
    #[serde(rename = "intent_version_id")]
    pub intent_version_id: Uuid,
    #[serde(rename = "type")]
    pub clause_type: ClauseType,
    #[serde(rename = "semantic_domain")]
    pub semantic_domain: String,
    pub key: String,
    pub operator: ConstraintOperator,
    pub value: serde_json::Value,
    pub priority: ClausePriority,
}

// -------------------------------
// Request/Response DTOs
// -------------------------------

/// Request to create a new intent
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreateIntentRequest {
    /// Tenant ID for quota enforcement (Phase 3 P3-S2).
    /// When None, quota enforcement is skipped on this path.
    /// Callers should populate this from auth context (JWT/API key).
    #[serde(rename = "tenant_id", default)]
    pub tenant_id: Option<Uuid>,
    #[serde(rename = "workflow_id")]
    pub workflow_id: Uuid,
    #[serde(rename = "source_refs")]
    pub source_refs: Vec<SourceRef>,
    #[validate(nested)]
    pub payload: IntentPayload,
    #[serde(rename = "created_by")]
    pub created_by: ActorRef,
    pub tags: Vec<String>,
}

/// Response after creating an intent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateIntentResponse {
    #[serde(rename = "intent_id")]
    pub intent_id: Uuid,
    #[serde(rename = "current_version")]
    pub current_version: i32,
    pub status: IntentStatus,
}

/// Request to create a new version
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateVersionRequest {
    pub payload: IntentPayload,
    #[serde(rename = "change_reason")]
    pub change_reason: String,
    #[serde(rename = "change_channel")]
    pub change_channel: ChangeChannel,
    #[serde(rename = "created_by")]
    pub created_by: ActorRef,
}

/// Response after creating a version
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateVersionResponse {
    #[serde(rename = "intent_version_id")]
    pub intent_version_id: Uuid,
    #[serde(rename = "intent_id")]
    pub intent_id: Uuid,
    #[serde(rename = "version_number")]
    pub version_number: i32,
    pub status: VersionStatus,
}

/// Response for getting intent head
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentHeadResponse {
    pub intent: Intent,
    pub version: IntentVersion,
    /// Row version for optimistic concurrency control.
    /// Clients should store this value and send back via X-Expected-Row-Version header
    /// when creating new versions to enable OCC detection.
    #[serde(rename = "row_version")]
    pub row_version: i32,
}

/// Response for listing versions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListVersionsResponse {
    pub intent_id: Uuid,
    pub versions: Vec<IntentVersion>,
    pub total: usize,
}

/// Request to compute diff between two versions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffRequest {
    pub from_version: i32,
    pub to_version: i32,
}

/// Response for intent validation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidateIntentResponse {
    pub valid: bool,
    pub errors: Vec<ValidationError>,
}

/// A single validation error
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationError {
    pub field: String,
    pub message: String,
}
