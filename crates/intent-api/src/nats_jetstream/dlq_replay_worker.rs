//! DLQ replay worker for replaying messages from DLQ to original subjects.
//!
//! Bounded helper decomposition slice (S6) from `nats_jetstream.rs`.
//! Polls a single DLQ subject at configured interval and replays messages
//! via `DlqHelper::replay_from_dlq()`.

use async_nats::jetstream::Context as JetStreamContext;
use std::time::Duration;
use tokio::sync::watch;
use tokio::time::timeout;

/// Configuration for DLQ replay worker
#[derive(Debug, Clone)]
pub struct DlqReplayWorkerConfig {
    /// DLQ subject to replay from
    pub dlq_subject: String,
    /// Poll interval between replay attempts
    pub poll_interval: Duration,
    /// Maximum messages to replay per poll (bounded to prevent overload)
    pub max_replay: usize,
}

impl DlqReplayWorkerConfig {
    /// Create a new config with default settings.
    pub fn new(dlq_subject: impl Into<String>) -> Self {
        Self {
            dlq_subject: dlq_subject.into(),
            poll_interval: Duration::from_secs(60),
            max_replay: 10,
        }
    }

    /// Set the poll interval.
    #[allow(dead_code)]
    pub fn with_poll_interval(mut self, interval: Duration) -> Self {
        self.poll_interval = interval;
        self
    }

    /// Set the maximum messages to replay per poll.
    #[allow(dead_code)]
    pub fn with_max_replay(mut self, max: usize) -> Self {
        self.max_replay = max;
        self
    }
}

impl Default for DlqReplayWorkerConfig {
    fn default() -> Self {
        Self::new("audit.events.v1.DLQ")
    }
}

/// Bounded DLQ replay worker for replaying messages from DLQ to original subjects.
///
/// Polls a single DLQ subject at configured interval and replays messages
/// via `DlqHelper::replay_from_dlq()`.
///
/// **Bounded behavior:**
/// - Creates a pull consumer with `AckPolicy::Explicit` on the DLQ subject
/// - Replays up to `max_replay` messages per poll
/// - ACKs DLQ message only after successful replay
/// - On replay failure, leaves message unacked and breaks the poll
/// - Graceful shutdown via watch channel
///
/// **Production Readiness:**
/// This is a BOUNDED FIRST SLICE. Not production-ready until G1-G5 gates pass.
#[derive(Debug)]
pub struct DlqReplayWorker {
    jetstream: JetStreamContext,
    config: DlqReplayWorkerConfig,
    stream_name: String,
    dlq_helper: super::dlq::DlqHelper,
}

impl DlqReplayWorker {
    /// Create a new DLQ replay worker.
    pub fn new(jetstream: JetStreamContext, config: DlqReplayWorkerConfig) -> Self {
        // Derive stream name from DLQ subject (same pattern as DlqMetricsWorker).
        let stream_name = config
            .dlq_subject
            .split('.')
            .nth(2)
            .map(|s| s.to_string())
            .unwrap_or_else(|| "audit_events".to_string());

        let dlq_helper = super::dlq::DlqHelper::new(jetstream.clone());

        Self {
            jetstream,
            config,
            stream_name,
            dlq_helper,
        }
    }

    /// Run the DLQ replay worker poll loop.
    ///
    /// **Bounded behavior:**
    /// - Polls DLQ subject at configured interval
    /// - Replays messages via `DlqHelper::replay_from_dlq()`
    /// - ACKs on success, leaves unacked on failure
    /// - Stops gracefully when shutdown signal is received
    pub async fn run(
        &self,
        mut shutdown: watch::Receiver<bool>,
    ) -> Result<(), DlqReplayWorkerError> {
        tracing::info!(
            "DlqReplayWorker: starting with subject '{}', poll_interval {:?}, max_replay {}",
            self.config.dlq_subject,
            self.config.poll_interval,
            self.config.max_replay
        );

        loop {
            // Check for shutdown signal
            if *shutdown.borrow() {
                tracing::info!("DlqReplayWorker: shutdown signal received, stopping poll loop");
                break;
            }

            // Attempt to replay messages from the DLQ subject
            match self.replay_dlq_subject(&self.config.dlq_subject).await {
                Ok(replayed) => {
                    if replayed > 0 {
                        tracing::info!("DlqReplayWorker: replayed {} messages", replayed);
                    }
                }
                Err(e) => {
                    tracing::warn!("DlqReplayWorker: replay poll failed: {}", e);
                }
            }

            // Wait for poll interval or shutdown
            let shutdown_fut = shutdown.changed();
            match timeout(self.config.poll_interval, shutdown_fut).await {
                Ok(Ok(())) => {
                    if *shutdown.borrow() {
                        tracing::info!(
                            "DlqReplayWorker: shutdown signal received, stopping poll loop"
                        );
                        break;
                    }
                }
                Ok(Err(_)) => {
                    tracing::warn!("DlqReplayWorker: shutdown channel closed unexpectedly");
                    break;
                }
                Err(_) => {
                    // Timeout — poll interval elapsed, continue to next poll
                }
            }
        }

        tracing::info!("DlqReplayWorker: poll loop stopped");
        Ok(())
    }

    /// Replay messages from a DLQ subject.
    ///
    /// Creates a temporary pull consumer to fetch messages, replays each one
    /// via `DlqHelper::replay_from_dlq()`, and ACKs only on successful replay.
    pub(super) async fn replay_dlq_subject(
        &self,
        dlq_subject: &str,
    ) -> Result<usize, DlqReplayWorkerError> {
        let consumer_name = format!("dlq_replay_{}", dlq_subject.replace('.', "_"));

        let consumer_config = async_nats::jetstream::consumer::pull::Config {
            durable_name: Some(consumer_name),
            description: Some("DLQ replay consumer".to_string()),
            filter_subject: dlq_subject.to_string(),
            ack_policy: async_nats::jetstream::consumer::AckPolicy::Explicit,
            max_deliver: 1,
            ..Default::default()
        };

        let consumer = match self
            .jetstream
            .create_consumer_on_stream(consumer_config, &self.stream_name)
            .await
        {
            Ok(c) => c,
            Err(e) => {
                return Err(DlqReplayWorkerError::ConsumerCreate(e.to_string()));
            }
        };

        let mut message_stream = match consumer.messages().await {
            Ok(stream) => stream,
            Err(e) => {
                return Err(DlqReplayWorkerError::FetchMessages(e.to_string()));
            }
        };

        let mut replayed = 0;

        use futures_util::StreamExt;

        while replayed < self.config.max_replay {
            match timeout(Duration::from_secs(1), message_stream.next()).await {
                Ok(Some(Ok(msg))) => {
                    match self.dlq_helper.replay_from_dlq(&msg).await {
                        Ok(()) => {
                            if let Err(e) = msg.ack().await {
                                tracing::warn!(
                                    "DlqReplayWorker: failed to ack replayed message: {}",
                                    e
                                );
                            }
                            replayed += 1;
                        }
                        Err(e) => {
                            tracing::error!(
                                "DlqReplayWorker: failed to replay message from '{}': {} — leaving unacked for manual investigation",
                                dlq_subject, e
                            );
                            // Do NOT ack — break to preserve ordering and leave message available
                            break;
                        }
                    }
                }
                Ok(Some(Err(e))) => {
                    tracing::warn!(
                        "DlqReplayWorker: error reading message from '{}': {}",
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
                    // Timeout waiting for next message — stop replaying
                    break;
                }
            }
        }

        Ok(replayed)
    }
}

/// Errors that can occur in the DLQ replay worker.
#[derive(Debug, Clone)]
pub enum DlqReplayWorkerError {
    /// Failed to create consumer for replaying
    ConsumerCreate(String),
    /// Failed to fetch messages from consumer
    FetchMessages(String),
}

impl std::fmt::Display for DlqReplayWorkerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DlqReplayWorkerError::ConsumerCreate(msg) => {
                write!(f, "DLQ replay worker: consumer create failed: {}", msg)
            }
            DlqReplayWorkerError::FetchMessages(msg) => {
                write!(f, "DLQ replay worker: fetch messages failed: {}", msg)
            }
        }
    }
}

impl std::error::Error for DlqReplayWorkerError {}

/// Handle to a running DLQ replay worker.
///
/// Allows graceful shutdown of the replay worker.
#[derive(Debug)]
pub struct DlqReplayWorkerHandle {
    handle: tokio::task::JoinHandle<Result<(), DlqReplayWorkerError>>,
    shutdown_tx: watch::Sender<bool>,
}

impl DlqReplayWorkerHandle {
    /// Signal the worker to stop gracefully.
    pub fn shutdown(&self) {
        tracing::info!("DlqReplayWorkerHandle: sending shutdown signal");
        let _ = self.shutdown_tx.send(true);
    }

    /// Wait for the worker to finish.
    pub async fn wait_for_all(self) {
        tracing::info!("DlqReplayWorkerHandle: waiting for worker to finish");
        match self.handle.await {
            Ok(Ok(())) => {
                tracing::info!("DlqReplayWorkerHandle: worker finished normally");
            }
            Ok(Err(e)) => {
                tracing::error!("DlqReplayWorkerHandle: worker failed: {:?}", e);
            }
            Err(e) => {
                tracing::error!("DlqReplayWorkerHandle: worker panicked: {:?}", e);
            }
        }
    }
}

/// Builder for DLQ replay worker with shutdown channel support.
///
/// Provides a convenient way to create and start a DLQ replay worker
/// with shared shutdown signaling.
pub struct DlqReplayWorkerBuilder {
    jetstream: JetStreamContext,
    config: DlqReplayWorkerConfig,
}

impl DlqReplayWorkerBuilder {
    /// Create a new builder with the given JetStream context and config.
    pub fn new(jetstream: JetStreamContext, config: DlqReplayWorkerConfig) -> Self {
        Self { jetstream, config }
    }

    /// Build and start the worker, returning a handle for shutdown.
    pub async fn start(self) -> Result<DlqReplayWorkerHandle, DlqReplayWorkerError> {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let worker = DlqReplayWorker::new(self.jetstream, self.config);

        let handle = tokio::spawn(async move { worker.run(shutdown_rx).await });

        Ok(DlqReplayWorkerHandle {
            handle,
            shutdown_tx,
        })
    }
}
