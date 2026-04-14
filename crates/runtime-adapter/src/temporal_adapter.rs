use crate::{
    AdapterError, AdapterResult, AdapterStatus, Checkpoint, CheckpointCandidate, IntentRef,
    RebaseSignal, RuntimeAdapter,
};
use async_trait::async_trait;
use futures_util::StreamExt;
use temporalio_client::{
    tonic::Request, Client, ClientOptions, Connection, ConnectionOptions, UntypedSignal,
    UntypedWorkflow, WorkflowDescribeOptions, WorkflowListOptions, WorkflowSignalOptions,
};
use temporalio_common::{
    data_converters::{PayloadConverter, RawValue},
    protos::grpc::health::v1::HealthCheckRequest,
    protos::temporal::api::enums::v1::WorkflowExecutionStatus,
};
use temporalio_sdk_core::Url;

/// Configuration for connecting to Temporal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemporalAdapterConfig {
    pub target_url: String,
    pub namespace: String,
    pub identity: String,
    pub workflow_query: String,
    pub max_checkpoints: usize,
}

impl TemporalAdapterConfig {
    pub fn validate(&self) -> AdapterResult<()> {
        if self.target_url.trim().is_empty() {
            return Err(AdapterError::NotReady(
                "Temporal target URL must not be empty".to_string(),
            ));
        }

        if self.namespace.trim().is_empty() {
            return Err(AdapterError::NotReady(
                "Temporal namespace must not be empty".to_string(),
            ));
        }

        if self.identity.trim().is_empty() {
            return Err(AdapterError::NotReady(
                "Temporal identity must not be empty".to_string(),
            ));
        }

        Ok(())
    }
}

impl Default for TemporalAdapterConfig {
    fn default() -> Self {
        Self {
            target_url: "http://localhost:7233".to_string(),
            namespace: "default".to_string(),
            identity: "intent-rebase-runtime-adapter".to_string(),
            workflow_query: "ExecutionStatus = 'Running'".to_string(),
            max_checkpoints: 25,
        }
    }
}

/// Real Temporal-backed adapter for runtime operations.
///
/// # Trace Propagation Limitation
///
/// Per-request gRPC trace metadata propagation (W3C `traceparent` / `tracestate`
/// injection into outbound Temporal gRPC calls) is **not supported** with the
/// current SDK version (`temporalio-client` 0.2.0).
///
/// - `Connection::set_headers()` mutates shared `Arc<RwLock<ClientHeaders>>`,
///   which is racy under concurrent use — using it would leak trace context
///   between concurrent requests.
/// - `WorkflowSignalOptions::header` sets Temporal workflow-level proto
///   headers, not gRPC transport metadata.
/// - `service_override` is a raw gRPC service interface that could theoretically
///   intercept requests, but is too invasive for this scope.
///
/// Local tracing span correlation is provided via `#[tracing::instrument]` on
/// all adapter methods. This limitation is tracked as a future-scope item.
#[derive(Clone)]
pub struct TemporalAdapter {
    client: Client,
    config: TemporalAdapterConfig,
}

impl TemporalAdapter {
    #[tracing::instrument(
        name = "temporal.connect",
        skip(config),
        fields(
            target_url = %config.target_url,
            namespace = %config.namespace
        )
    )]
    pub async fn connect(config: TemporalAdapterConfig) -> AdapterResult<Self> {
        config.validate()?;

        let url = config
            .target_url
            .parse::<Url>()
            .map_err(|e| {
                tracing::warn!(error = %e, "Invalid Temporal URL");
                AdapterError::NotReady(format!("Invalid Temporal URL: {e}"))
            })?;

        let connection = Connection::connect(
            ConnectionOptions::new(url)
                .identity(config.identity.clone())
                .build(),
        )
        .await
        .map_err(|e| {
            tracing::warn!(error = %e, "Failed to connect to Temporal");
            AdapterError::NotReady(format!("Failed to connect to Temporal: {e}"))
        })?;

        let client = Client::new(
            connection,
            ClientOptions::new(config.namespace.clone()).build(),
        )
        .map_err(|e| {
            tracing::error!(error = %e, "Failed to build Temporal client");
            AdapterError::Internal(format!("Failed to build Temporal client: {e}"))
        })?;

        tracing::info!(namespace = %config.namespace, "Connected to Temporal");
        Ok(Self { client, config })
    }

    pub fn from_client(client: Client, config: TemporalAdapterConfig) -> Self {
        Self { client, config }
    }

    fn workflow_id_from_signal(signal: &RebaseSignal) -> AdapterResult<String> {
        signal
            .metadata
            .get("workflow_id")
            .and_then(|value| value.as_str())
            .map(ToOwned::to_owned)
            .ok_or_else(|| {
                AdapterError::RebaseSignalFailed(
                    "Missing workflow_id in rebase signal metadata".to_string(),
                )
            })
    }

    fn checkpoint_candidate_from_execution(
        execution: temporalio_client::WorkflowExecution,
    ) -> CheckpointCandidate {
        let workflow_id = execution.id().to_string();
        let workflow_type = execution.workflow_type().to_string();

        CheckpointCandidate {
            id: workflow_id.clone(),
            label: workflow_type.clone(),
            description: format!(
                "Temporal workflow candidate {workflow_id} ({workflow_type}) discovered via visibility query"
            ),
            validated: false,
        }
    }

    fn describe_checkpoint_candidate(
        intent: &IntentRef,
        workflow: temporalio_client::WorkflowExecution,
    ) -> AdapterResult<CheckpointCandidate> {
        let workflow_id = workflow.id().trim();
        if workflow_id.is_empty() {
            return Err(AdapterError::CheckpointNotFound(format!(
                "Temporal workflow description for intent {} did not include a workflow ID",
                intent.id
            )));
        }

        let status = workflow.status();
        if status.as_str_name() != "WORKFLOW_EXECUTION_STATUS_RUNNING" {
            return Err(AdapterError::CheckpointNotFound(format!(
                "Temporal workflow {} for intent {} is not replayable (status: {})",
                workflow_id,
                intent.id,
                Self::workflow_status_label(status),
            )));
        }

        let run_id = workflow.run_id().trim();
        let workflow_type = workflow.workflow_type().trim();
        let correlated_via_metadata =
            workflow.memo().is_some() || workflow.search_attributes().is_some();
        let candidate_id = if run_id.is_empty() {
            workflow_id.to_string()
        } else {
            format!("{workflow_id}:{run_id}")
        };
        let label = if workflow_type.is_empty() {
            "Temporal workflow".to_string()
        } else {
            workflow_type.to_string()
        };
        let correlation_hint = if correlated_via_metadata {
            "describe metadata present"
        } else {
            "workflow-id-only correlation"
        };

        Ok(CheckpointCandidate {
            id: candidate_id,
            label,
            description: format!(
                "Temporal workflow {} mapped from intent {} using describe() (status: {}, {})",
                workflow_id,
                intent.id,
                Self::workflow_status_label(status),
                correlation_hint,
            ),
            validated: true,
        })
    }

    fn replay_signal(intent: &IntentRef, checkpoint: &Checkpoint) -> RebaseSignal {
        RebaseSignal {
            intent_id: intent.id.clone(),
            signal_type: "replay".to_string(),
            metadata: serde_json::json!({
                "workflow_id": intent.workflow_id,
                "tenant_id": intent.tenant_id,
                "checkpoint_id": checkpoint.id,
                "checkpoint_label": checkpoint.label,
                "checkpoint_description": checkpoint.description,
                "checkpoint_timestamp": checkpoint.timestamp.to_rfc3339(),
                "checkpoint_validated": checkpoint.validated,
                "replay_mode": "cooperative_signal",
                "replay_reason": "phase-2b-batch-2-checkpoint-replay",
            }),
        }
    }

    fn workflow_status_label(status: WorkflowExecutionStatus) -> &'static str {
        match status.as_str_name() {
            "WORKFLOW_EXECUTION_STATUS_UNSPECIFIED" => "unspecified",
            "WORKFLOW_EXECUTION_STATUS_RUNNING" => "running",
            "WORKFLOW_EXECUTION_STATUS_COMPLETED" => "completed",
            "WORKFLOW_EXECUTION_STATUS_FAILED" => "failed",
            "WORKFLOW_EXECUTION_STATUS_CANCELED" => "canceled",
            "WORKFLOW_EXECUTION_STATUS_TERMINATED" => "terminated",
            "WORKFLOW_EXECUTION_STATUS_CONTINUED_AS_NEW" => "continued_as_new",
            "WORKFLOW_EXECUTION_STATUS_TIMED_OUT" => "timed_out",
            other => other,
        }
    }
}

#[async_trait]
impl RuntimeAdapter for TemporalAdapter {
    #[tracing::instrument(
        name = "temporal.get_checkpoints",
        skip(self),
        fields(
            workflow_query = %self.config.workflow_query,
            max_checkpoints = self.config.max_checkpoints
        )
    )]
    async fn get_checkpoints(&self) -> AdapterResult<Vec<CheckpointCandidate>> {
        let mut stream = self.client.list_workflows(
            self.config.workflow_query.clone(),
            WorkflowListOptions::builder()
                .limit(self.config.max_checkpoints)
                .build(),
        );

        let mut checkpoints = Vec::new();
        while let Some(result) = stream.next().await {
            let execution = result.map_err(|e| {
                tracing::warn!(error = %e, "Failed to list Temporal workflow");
                AdapterError::Internal(format!("Failed to list Temporal workflows: {e}"))
            })?;
            checkpoints.push(Self::checkpoint_candidate_from_execution(execution));
        }

        tracing::info!(count = checkpoints.len(), "Listed Temporal workflows");
        Ok(checkpoints)
    }

    #[tracing::instrument(
        name = "temporal.send_rebase_signal",
        skip(self, signal),
        fields(
            intent_id = %signal.intent_id,
            signal_type = %signal.signal_type
        )
    )]
    async fn send_rebase_signal(&self, signal: RebaseSignal) -> AdapterResult<()> {
        let workflow_id = Self::workflow_id_from_signal(&signal)?;
        let handle = self
            .client
            .get_workflow_handle::<UntypedWorkflow>(workflow_id.clone());
        let payload_converter = PayloadConverter::serde_json();
        let raw_signal = RawValue::from_value(&signal, &payload_converter);

        handle
            .signal(
                UntypedSignal::new(signal.signal_type.clone()),
                raw_signal,
                WorkflowSignalOptions::default(),
            )
            .await
            .map_err(|e| {
                tracing::warn!(workflow_id = %workflow_id, error = %e, "Failed to send Temporal rebase signal");
                AdapterError::RebaseSignalFailed(format!(
                    "Failed to send Temporal rebase signal: {e}"
                ))
            })?;

        tracing::info!(workflow_id = %workflow_id, "Temporal rebase signal sent");
        Ok(())
    }

    #[tracing::instrument(
        name = "temporal.map_intent_to_checkpoint",
        skip(self),
        fields(
            intent_id = %intent.id,
            workflow_id = %intent.workflow_id
        )
    )]
    async fn map_intent_to_checkpoint(
        &self,
        intent: IntentRef,
    ) -> AdapterResult<CheckpointCandidate> {
        let handle = self
            .client
            .get_workflow_handle::<UntypedWorkflow>(intent.workflow_id.clone());
        let description = handle
            .describe(WorkflowDescribeOptions::default())
            .await
            .map_err(|e| {
                tracing::warn!(
                    workflow_id = %intent.workflow_id,
                    intent_id = %intent.id,
                    error = %e,
                    "Failed to describe Temporal workflow"
                );
                AdapterError::IntentMappingFailed(format!(
                    "Failed to describe Temporal workflow {} for intent {}: {e}",
                    intent.workflow_id, intent.id
                ))
            })?;

        let workflow_info = description
            .raw_description
            .workflow_execution_info
            .ok_or_else(|| {
                tracing::warn!(
                    intent_id = %intent.id,
                    "Temporal describe response missing workflow execution info"
                );
                AdapterError::IntentMappingFailed(format!(
                    "Temporal describe response for intent {} did not include workflow execution info",
                    intent.id
                ))
            })?;

        let candidate = Self::describe_checkpoint_candidate(
            &intent,
            temporalio_client::WorkflowExecution::new(workflow_info),
        )
        .map_err(|e| match e {
            AdapterError::CheckpointNotFound(message)
            | AdapterError::IntentMappingFailed(message) => {
                tracing::warn!(intent_id = %intent.id, error = %message, "Checkpoint candidate not found");
                AdapterError::IntentMappingFailed(message)
            }
            other => AdapterError::IntentMappingFailed(other.to_string()),
        })?;

        tracing::info!(
            intent_id = %intent.id,
            checkpoint_id = %candidate.id,
            "Mapped intent to Temporal checkpoint"
        );
        Ok(candidate)
    }

    #[tracing::instrument(
        name = "temporal.replay_from_checkpoint",
        skip(self),
        fields(
            intent_id = %intent.id,
            checkpoint_id = %checkpoint.id
        )
    )]
    async fn replay_from_checkpoint(
        &self,
        checkpoint: Checkpoint,
        intent: IntentRef,
    ) -> AdapterResult<()> {
        let replay_signal = Self::replay_signal(&intent, &checkpoint);
        self.send_rebase_signal(replay_signal).await.map_err(|e| {
            tracing::error!(
                intent_id = %intent.id,
                checkpoint_id = %checkpoint.id,
                error = %e,
                "Failed to trigger cooperative Temporal replay"
            );
            AdapterError::ReplayFailed(format!(
                "Failed to trigger cooperative Temporal replay for checkpoint {} and intent {}: {}",
                checkpoint.id, intent.id, e
            ))
        })
    }

    #[tracing::instrument(
        name = "temporal.is_adapter_ready",
        skip(self)
    )]
    async fn is_adapter_ready(&self) -> AdapterResult<AdapterStatus> {
        let mut service = self.client.connection().health_service();
        service
            .check(Request::new(HealthCheckRequest {
                service: String::new(),
            }))
            .await
            .map(|_| {
                tracing::debug!("Temporal adapter health check passed");
                AdapterStatus::Ready
            })
            .map_err(|e| {
                tracing::warn!(error = %e, "Temporal health check failed");
                AdapterError::NotReady(format!("Temporal health check failed: {e}"))
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn create_test_runtime_checkpoint() -> Checkpoint {
        Checkpoint {
            id: "checkpoint-1".to_string(),
            label: "Checkpoint 1".to_string(),
            description: "Test checkpoint".to_string(),
            timestamp: chrono::Utc::now(),
            validated: true,
        }
    }

    fn create_test_runtime_intent() -> IntentRef {
        IntentRef::new(
            "intent-1".to_string(),
            "tenant-1".to_string(),
            "wf-123".to_string(),
            "active".to_string(),
        )
    }

    #[test]
    fn test_temporal_adapter_config_validation() {
        let valid = TemporalAdapterConfig::default();
        assert!(valid.validate().is_ok());

        let invalid = TemporalAdapterConfig {
            target_url: String::new(),
            ..TemporalAdapterConfig::default()
        };
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn test_temporal_signal_requires_workflow_id_metadata() {
        let signal = RebaseSignal {
            intent_id: "intent-1".to_string(),
            signal_type: "proceed".to_string(),
            metadata: json!({}),
        };

        let err = TemporalAdapter::workflow_id_from_signal(&signal).unwrap_err();
        assert!(matches!(err, AdapterError::RebaseSignalFailed(_)));
    }

    #[test]
    fn test_temporal_signal_extracts_workflow_id() {
        let signal = RebaseSignal {
            intent_id: "intent-1".to_string(),
            signal_type: "proceed".to_string(),
            metadata: json!({"workflow_id": "wf-123"}),
        };

        let workflow_id = TemporalAdapter::workflow_id_from_signal(&signal).unwrap();
        assert_eq!(workflow_id, "wf-123");
    }

    #[test]
    fn test_temporal_signal_roundtrip_serialization() {
        let signal = RebaseSignal {
            intent_id: "intent-1".to_string(),
            signal_type: "proceed".to_string(),
            metadata: json!({"workflow_id": "wf-123", "checkpoint_id": "cp-1"}),
        };

        let encoded = serde_json::to_string(&signal).unwrap();
        let decoded: RebaseSignal = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded.intent_id, signal.intent_id);
        assert_eq!(decoded.signal_type, signal.signal_type);
        assert_eq!(decoded.metadata["workflow_id"], "wf-123");
    }

    #[test]
    fn test_temporal_replay_signal_contains_checkpoint_metadata() {
        let checkpoint = create_test_runtime_checkpoint();
        let intent = create_test_runtime_intent();

        let signal = TemporalAdapter::replay_signal(&intent, &checkpoint);

        assert_eq!(signal.signal_type, "replay");
        assert_eq!(signal.intent_id, intent.id);
        assert_eq!(signal.metadata["workflow_id"], intent.workflow_id);
        assert_eq!(signal.metadata["checkpoint_id"], checkpoint.id);
        assert_eq!(signal.metadata["checkpoint_label"], checkpoint.label);
        assert_eq!(signal.metadata["replay_mode"], "cooperative_signal");
    }

    #[test]
    fn test_temporal_status_label_maps_known_values() {
        assert_eq!(
            TemporalAdapter::workflow_status_label(WorkflowExecutionStatus::Running),
            "running"
        );
        assert_eq!(
            TemporalAdapter::workflow_status_label(WorkflowExecutionStatus::Completed),
            "completed"
        );
    }
}
