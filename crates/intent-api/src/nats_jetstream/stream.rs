//! JetStream stream initialization (idempotent, fail-safe).
//!
//! Bounded helper decomposition slice (S3) from `nats_jetstream.rs`.
//! Creates a single stream `audit_events` for subject `audit.events.v1.>`
//! with bounded configuration (no replication/cluster).

use async_nats::jetstream::Context as JetStreamContext;
use std::time::Duration;
use tokio::time::timeout;

/// Phase 3: JetStream stream initialization for audit events.
///
/// Creates a single stream `audit_events` for subject `audit.events.v1.>`
/// with bounded configuration (no replication/cluster).
///
/// ## Idempotent Behavior
///
/// Uses `get_or_create_stream` - if the stream already exists, this is a no-op.
/// This means multiple service restarts will not create duplicate streams.
///
/// ## Fail-Safe Behavior
///
/// If NATS is unavailable at startup, this logs a warning and returns an error.
/// The service can continue without JetStream - this is intentional for bounded
/// Phase 3: NATS unavailability should not crash the service unless live
/// integration tests explicitly require it.
///
/// ## Stream Configuration
///
/// - **Name**: `audit_events`
/// - **Subjects**: `audit.events.v1.>`
/// - **Retention**: default (keep messages until consumed/expired)
/// - **No replication**: single-node bounded scope
pub struct JetStreamInitializer {
    /// Stream name
    stream_name: &'static str,
    /// Subject filter
    subject_filter: &'static str,
    /// Connect timeout for JetStream
    connect_timeout: Duration,
}

impl JetStreamInitializer {
    /// Create a new JetStream initializer with default settings.
    pub fn new() -> Self {
        Self {
            stream_name: "audit_events",
            subject_filter: "audit.events.v1.>",
            connect_timeout: Duration::from_secs(5),
        }
    }

    /// Create with custom settings.
    #[allow(dead_code)]
    pub fn with_settings(stream_name: &'static str, subject_filter: &'static str) -> Self {
        Self {
            stream_name,
            subject_filter,
            connect_timeout: Duration::from_secs(5),
        }
    }

    /// Get the stream name.
    pub fn stream_name(&self) -> &'static str {
        self.stream_name
    }

    /// Get the subject filter.
    pub fn subject_filter(&self) -> &'static str {
        self.subject_filter
    }

    /// Initialize JetStream context and ensure stream exists.
    ///
    /// Returns `Ok(Context)` if stream exists or was created.
    /// Returns `Err` if NATS is unavailable or stream creation fails.
    ///
    /// **Fail-safe**: Returns error instead of panicking if NATS is unavailable.
    /// Callers can handle this gracefully (log warning, continue without JetStream).
    pub async fn ensure_stream(&self, nats_url: &str) -> Result<JetStreamContext, String> {
        tracing::info!(
            "Connecting to NATS at {} for JetStream initialization",
            nats_url
        );

        // Connect to NATS with timeout
        let client = timeout(self.connect_timeout, async_nats::connect(nats_url))
            .await
            .map_err(|_| format!("NATS connection timed out after {:?}", self.connect_timeout))?
            .map_err(|e| format!("NATS connection failed: {}", e))?;

        // Create JetStream context
        let jetstream = async_nats::jetstream::new(client);

        // Try to create or get the stream
        self.get_or_create_stream(&jetstream).await?;

        Ok(jetstream)
    }

    /// Get or create the audit events stream idempotently.
    async fn get_or_create_stream(&self, jetstream: &JetStreamContext) -> Result<(), String> {
        use async_nats::jetstream::stream::Config;

        let config = Config {
            name: self.stream_name.to_string(),
            subjects: vec![self.subject_filter.to_string()],
            ..Default::default()
        };

        match jetstream.get_or_create_stream(config).await {
            Ok(_stream) => {
                tracing::info!(
                    "JetStream stream '{}' ready (subject: {})",
                    self.stream_name,
                    self.subject_filter
                );
                Ok(())
            }
            Err(e) => {
                let err_msg = format!("Failed to create JetStream stream: {}", e);
                tracing::error!("{}", err_msg);
                Err(err_msg)
            }
        }
    }
}

impl Default for JetStreamInitializer {
    fn default() -> Self {
        Self::new()
    }
}
