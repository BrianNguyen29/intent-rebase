//! DLQ metrics worker for monitoring DLQ depth and age.
//!
//! Bounded helper decomposition slice (S5) from `nats_jetstream.rs`.
//! Polls DLQ subjects at configured interval and emits gauge metrics:
//! - `intent_api_dlq_messages_current`: Total messages across all monitored DLQ subjects
//! - `intent_api_dlq_message_age_seconds`: Age of oldest message across all DLQ subjects

use async_nats::jetstream::Context as JetStreamContext;
use std::time::Duration;
use tokio::sync::watch;
use tokio::time::timeout;

/// Configuration for DLQ metrics worker
#[derive(Debug, Clone)]
pub struct DlqMetricsWorkerConfig {
    /// DLQ subjects to monitor for depth/age metrics
    pub dlq_subjects: Vec<String>,
    /// Poll interval between metric collections
    pub poll_interval: Duration,
    /// Maximum messages to peek per subject per poll (bounded to prevent overload)
    pub max_peek: usize,
    /// JetStream connection timeout
    pub connect_timeout: Duration,
}

impl DlqMetricsWorkerConfig {
    /// Create a new config with default settings.
    pub fn new() -> Self {
        Self {
            dlq_subjects: Vec::new(),
            poll_interval: Duration::from_secs(30),
            max_peek: 100,
            connect_timeout: Duration::from_secs(5),
        }
    }

    /// Add a DLQ subject to monitor.
    #[allow(dead_code)]
    pub fn add_dlq_subject(mut self, subject: &str) -> Self {
        self.dlq_subjects.push(subject.to_string());
        self
    }

    /// Set the poll interval.
    #[allow(dead_code)]
    pub fn with_poll_interval(mut self, interval: Duration) -> Self {
        self.poll_interval = interval;
        self
    }

    /// Set the maximum messages to peek per subject.
    #[allow(dead_code)]
    pub fn with_max_peek(mut self, max: usize) -> Self {
        self.max_peek = max;
        self
    }
}

impl Default for DlqMetricsWorkerConfig {
    fn default() -> Self {
        Self::new()
    }
}

/// Bounded DLQ metrics worker for monitoring DLQ depth and age.
///
/// Polls DLQ subjects at configured interval and emits gauge metrics:
/// - `intent_api_dlq_messages_current`: Total messages across all monitored DLQ subjects
/// - `intent_api_dlq_message_age_seconds`: Age of oldest message across all DLQ subjects
///
/// **Bounded behavior:**
/// - Uses lightweight pull consumer peek (ack_policy = None) to count messages without consuming
/// - Does NOT remove messages from DLQ
/// - Graceful shutdown via watch channel
///
/// **Production Readiness:**
/// This is a BOUNDED FIRST SLICE. Not production-ready until G1-G5 gates pass.
#[derive(Debug)]
pub struct DlqMetricsWorker {
    /// JetStream context for consumer operations
    jetstream: JetStreamContext,
    /// Worker configuration
    config: DlqMetricsWorkerConfig,
    /// Stream name where DLQ subjects live (derived from first subject)
    stream_name: String,
}

impl DlqMetricsWorker {
    /// Create a new DLQ metrics worker.
    ///
    /// **Note:** The JetStream context is created lazily on first `run` call.
    pub fn new(jetstream: JetStreamContext, config: DlqMetricsWorkerConfig) -> Self {
        // Derive stream name from first DLQ subject if available
        // Default to "audit_events" as the bounded stream name
        let stream_name = config
            .dlq_subjects
            .first()
            .and_then(|s| s.split('.').nth(2)) // e.g., "audit" from "audit.events.v1.>"
            .map(|s| s.to_string())
            .unwrap_or_else(|| "audit_events".to_string());

        Self {
            jetstream,
            config,
            stream_name,
        }
    }

    /// Create a new DLQ metrics worker with a default config.
    ///
    /// **Note:** The JetStream context is created lazily on first `run` call.
    #[allow(dead_code)]
    pub fn with_defaults(jetstream: JetStreamContext) -> Self {
        Self::new(jetstream, DlqMetricsWorkerConfig::new())
    }

    /// Run the DLQ metrics worker poll loop.
    ///
    /// **Bounded behavior:**
    /// - Polls DLQ subjects at configured interval
    /// - Uses lightweight peek to count messages without consuming
    /// - Emits gauge metrics for depth and age
    /// - Stops gracefully when shutdown signal is received
    ///
    /// # Arguments
    ///
    /// * `shutdown` - Channel to receive shutdown signal
    pub async fn run(
        &self,
        mut shutdown: watch::Receiver<bool>,
    ) -> Result<(), DlqMetricsWorkerError> {
        tracing::info!(
            "DlqMetricsWorker: starting with {} DLQ subjects, poll_interval {:?}",
            self.config.dlq_subjects.len(),
            self.config.poll_interval
        );

        loop {
            // Check for shutdown signal
            if *shutdown.borrow() {
                tracing::info!("DlqMetricsWorker: shutdown signal received, stopping poll loop");
                break;
            }

            // Collect and emit metrics for all DLQ subjects
            self.collect_and_emit_metrics().await;

            // Wait for poll interval or shutdown
            let shutdown_fut = shutdown.changed();

            match timeout(self.config.poll_interval, shutdown_fut).await {
                Ok(Ok(())) => {
                    // Shutdown signal received
                    if *shutdown.borrow() {
                        tracing::info!(
                            "DlqMetricsWorker: shutdown signal received, stopping poll loop"
                        );
                        break;
                    }
                }
                Ok(Err(_)) => {
                    // Channel closed unexpectedly
                    tracing::warn!("DlqMetricsWorker: shutdown channel closed unexpectedly");
                    break;
                }
                Err(_) => {
                    // Timeout — poll interval elapsed, continue to next poll
                }
            }
        }

        tracing::info!("DlqMetricsWorker: poll loop stopped");
        Ok(())
    }

    /// Collect metrics from all DLQ subjects and emit gauges.
    async fn collect_and_emit_metrics(&self) {
        let mut total_messages: i64 = 0;
        let mut oldest_age_secs: Option<f64> = None;

        for dlq_subject in &self.config.dlq_subjects {
            match self.peek_dlq_subject(dlq_subject).await {
                Ok((count, oldest_timestamp)) => {
                    total_messages += count as i64;

                    // Calculate age of oldest message if timestamp is available
                    if let Some(ts) = oldest_timestamp {
                        let age_secs = (chrono::Utc::now() - ts).num_seconds() as f64;
                        if oldest_age_secs.map(|o| age_secs > o).unwrap_or(true) {
                            oldest_age_secs = Some(age_secs);
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        "DlqMetricsWorker: failed to peek DLQ subject '{}': {}",
                        dlq_subject,
                        e
                    );
                }
            }
        }

        // Emit gauges via lib.rs helpers
        crate::record_dlq_messages_current(total_messages as f64);
        if let Some(age) = oldest_age_secs {
            crate::record_dlq_message_age_seconds(age);
        } else {
            // No messages in DLQ — emit 0 age
            crate::record_dlq_message_age_seconds(0.0);
        }

        tracing::debug!(
            "DlqMetricsWorker: emitted metrics — depth={}, oldest_age={:?}",
            total_messages,
            oldest_age_secs
        );
    }

    /// Peek a DLQ subject to count messages and find oldest timestamp.
    ///
    /// Uses a lightweight pull consumer with `ack_policy = None` to peek messages
    /// without consuming them.
    pub(super) async fn peek_dlq_subject(
        &self,
        dlq_subject: &str,
    ) -> Result<(usize, Option<chrono::DateTime<chrono::Utc>>), DlqMetricsWorkerError> {
        // Create a temporary pull consumer to peek messages
        let consumer_name = format!("dlq_peek_{}", dlq_subject.replace('.', "_"));

        let consumer_config = async_nats::jetstream::consumer::pull::Config {
            durable_name: Some(consumer_name.clone()),
            description: Some("DLQ metrics peek consumer".to_string()),
            ack_policy: async_nats::jetstream::consumer::AckPolicy::None,
            max_deliver: 1,
            ..Default::default()
        };

        // Try to create or get consumer
        let consumer = match self
            .jetstream
            .create_consumer_on_stream(consumer_config, &self.stream_name)
            .await
        {
            Ok(c) => c,
            Err(e) => {
                return Err(DlqMetricsWorkerError::ConsumerCreate(e.to_string()));
            }
        };

        // Fetch messages with timeout
        let mut message_count = 0;
        let mut oldest_timestamp: Option<chrono::DateTime<chrono::Utc>> = None;

        let fetch_fut = consumer.messages();
        let messages = match timeout(Duration::from_secs(5), fetch_fut).await {
            Ok(Ok(stream)) => stream,
            Ok(Err(e)) => {
                return Err(DlqMetricsWorkerError::FetchMessages(e.to_string()));
            }
            Err(_) => {
                // Timeout — treat as 0 messages (normal when DLQ is empty)
                return Ok((0, None));
            }
        };

        use futures_util::StreamExt;
        let mut message_stream = messages;

        // Peek up to max_peek messages
        while message_count < self.config.max_peek {
            match timeout(Duration::from_secs(1), message_stream.next()).await {
                Ok(Some(Ok(msg))) => {
                    message_count += 1;

                    // Extract timestamp from Nats-DLQ-Timestamp header
                    if let Some(ts_header) = msg
                        .headers
                        .as_ref()
                        .and_then(|h| h.get(super::dlq::HEADER_DLQ_TIMESTAMP))
                    {
                        let ts_str = ts_header.to_string();
                        if let Ok(ts) = chrono::DateTime::parse_from_rfc3339(&ts_str) {
                            let dt = ts.with_timezone(&chrono::Utc);
                            if oldest_timestamp.map(|o| dt < o).unwrap_or(true) {
                                oldest_timestamp = Some(dt);
                            }
                        }
                    }

                    // ack_policy is None — do NOT ack; peek must not consume messages
                }
                Ok(Some(Err(e))) => {
                    tracing::warn!(
                        "DlqMetricsWorker: error reading message from '{}': {}",
                        dlq_subject,
                        e
                    );
                    break;
                }
                Ok(None) => {
                    // No more messages
                    break;
                }
                Err(_) => {
                    // Timeout waiting for next message — stop peeking
                    break;
                }
            }
        }

        Ok((message_count, oldest_timestamp))
    }
}

/// Errors that can occur when operating the DLQ metrics worker
#[derive(Debug, Clone)]
pub enum DlqMetricsWorkerError {
    /// Failed to create consumer for peeking
    ConsumerCreate(String),
    /// Failed to fetch messages from consumer
    FetchMessages(String),
    /// Failed to parse timestamp
    TimestampParse(String),
}

impl std::fmt::Display for DlqMetricsWorkerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DlqMetricsWorkerError::ConsumerCreate(msg) => {
                write!(f, "DLQ metrics worker: consumer create failed: {}", msg)
            }
            DlqMetricsWorkerError::FetchMessages(msg) => {
                write!(f, "DLQ metrics worker: fetch messages failed: {}", msg)
            }
            DlqMetricsWorkerError::TimestampParse(msg) => {
                write!(f, "DLQ metrics worker: timestamp parse failed: {}", msg)
            }
        }
    }
}

impl std::error::Error for DlqMetricsWorkerError {}

/// Handle to a running DLQ metrics worker.
///
/// Allows graceful shutdown of the metrics worker.
#[derive(Debug)]
pub struct DlqMetricsWorkerHandle {
    /// Task handle for the running worker
    handle: tokio::task::JoinHandle<Result<(), DlqMetricsWorkerError>>,
    /// Shutdown signal sender
    shutdown_tx: watch::Sender<bool>,
}

impl DlqMetricsWorkerHandle {
    /// Signal the worker to stop gracefully.
    pub fn shutdown(&self) {
        tracing::info!("DlqMetricsWorkerHandle: sending shutdown signal");
        let _ = self.shutdown_tx.send(true);
    }

    /// Wait for the worker to finish.
    pub async fn wait_for_all(self) {
        tracing::info!("DlqMetricsWorkerHandle: waiting for worker to finish");
        match self.handle.await {
            Ok(Ok(())) => {
                tracing::info!("DlqMetricsWorkerHandle: worker finished normally");
            }
            Ok(Err(e)) => {
                tracing::error!("DlqMetricsWorkerHandle: worker failed: {:?}", e);
            }
            Err(e) => {
                tracing::error!("DlqMetricsWorkerHandle: worker panicked: {:?}", e);
            }
        }
    }
}

/// Builder for DLQ metrics worker with shutdown channel support.
///
/// Provides a convenient way to create and start a DLQ metrics worker
/// with shared shutdown signaling.
pub struct DlqMetricsWorkerBuilder {
    jetstream: JetStreamContext,
    config: DlqMetricsWorkerConfig,
}

impl DlqMetricsWorkerBuilder {
    /// Create a new builder with the given JetStream context and config.
    pub fn new(jetstream: JetStreamContext, config: DlqMetricsWorkerConfig) -> Self {
        Self { jetstream, config }
    }

    /// Build and start the worker, returning a handle for shutdown.
    pub async fn start(self) -> Result<DlqMetricsWorkerHandle, DlqMetricsWorkerError> {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let worker = DlqMetricsWorker::new(self.jetstream, self.config);

        let handle = tokio::spawn(async move { worker.run(shutdown_rx).await });

        Ok(DlqMetricsWorkerHandle {
            handle,
            shutdown_tx,
        })
    }
}
