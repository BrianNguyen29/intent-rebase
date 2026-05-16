//! DLQ error types, NATS subject validation, and DLQ helper.
//!
//! Bounded helper decomposition slices (S2 + S4) from `nats_jetstream.rs`.
//! Contains error types for DLQ subject derivation, publishing, and replay,
//! `validate_nats_subject` for checking NATS subject compliance,
//! DLQ header constants, and the `DlqHelper` struct for explicit DLQ
//! subject derivation and message routing primitives.

use async_nats::jetstream::Context as JetStreamContext;

/// Errors that can occur when deriving a DLQ subject
#[derive(Debug, Clone)]
pub enum DlqSubjectError {
    /// Subject was empty
    EmptySubject,
    /// Subject exceeded NATS protocol limit (1024 bytes)
    SubjectTooLong { length: usize, max: usize },
    /// Subject contains invalid NATS characters
    InvalidCharacters { invalid_char: char, position: usize },
    /// Subject contains empty token (consecutive dots or leading/trailing dot)
    EmptyToken { position: usize },
    /// Token exceeded maximum length (255 bytes)
    TokenTooLong {
        length: usize,
        max: usize,
        position: usize,
    },
}

impl std::fmt::Display for DlqSubjectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DlqSubjectError::EmptySubject => {
                write!(
                    f,
                    "DLQ subject derivation failed: original subject is empty"
                )
            }
            DlqSubjectError::SubjectTooLong { length, max } => {
                write!(
                    f,
                    "DLQ subject derivation failed: subject length {} exceeds NATS limit {}",
                    length, max
                )
            }
            DlqSubjectError::InvalidCharacters {
                invalid_char,
                position,
            } => {
                write!(
                    f,
                    "DLQ subject derivation failed: invalid NATS character '{}' at position {}",
                    invalid_char, position
                )
            }
            DlqSubjectError::EmptyToken { position } => {
                write!(
                    f,
                    "DLQ subject derivation failed: empty token at position {}",
                    position
                )
            }
            DlqSubjectError::TokenTooLong {
                length,
                max,
                position,
            } => {
                write!(
                    f,
                    "DLQ subject derivation failed: token length {} exceeds NATS token limit {} at position {}",
                    length, max, position
                )
            }
        }
    }
}

impl std::error::Error for DlqSubjectError {}

/// Errors that can occur when publishing to DLQ
#[derive(Debug, Clone)]
pub enum DlqPublishError {
    /// Failed to derive DLQ subject from original subject
    SubjectDerivation(String),
    /// Failed to publish message to DLQ subject
    PublishFailed(String),
}

impl std::fmt::Display for DlqPublishError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DlqPublishError::SubjectDerivation(msg) => {
                write!(f, "DLQ publish failed: subject derivation error: {}", msg)
            }
            DlqPublishError::PublishFailed(msg) => {
                write!(f, "DLQ publish failed: publish error: {}", msg)
            }
        }
    }
}

impl std::error::Error for DlqPublishError {}

/// Errors that can occur when replaying from DLQ
#[derive(Debug, Clone)]
pub enum DlqReplayError {
    /// Missing `Nats-Orig-Subject` header in DLQ message
    MissingOrigSubjectHeader,
    /// Target subject is invalid
    InvalidSubject(String),
    /// Failed to publish message during replay
    PublishFailed(String),
}

impl std::fmt::Display for DlqReplayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DlqReplayError::MissingOrigSubjectHeader => {
                write!(
                    f,
                    "DLQ replay failed: missing required header '{}'",
                    HEADER_ORIG_SUBJECT
                )
            }
            DlqReplayError::InvalidSubject(msg) => {
                write!(f, "DLQ replay failed: invalid subject: {}", msg)
            }
            DlqReplayError::PublishFailed(msg) => {
                write!(f, "DLQ replay failed: publish error: {}", msg)
            }
        }
    }
}

impl std::error::Error for DlqReplayError {}

// =============================================================================
// NATS Subject Validation
// =============================================================================

/// Validate a string as a valid NATS subject.
///
/// NATS subject rules:
/// - Non-empty tokens separated by dots (`.`)
/// - No whitespace, null bytes, or special metacharacters (`*`, `>`, `\`)
/// - Max token length: 255 bytes
/// - Max subject length: 1024 bytes
pub(super) fn validate_nats_subject(subject: &str) -> Result<(), DlqSubjectError> {
    if subject.is_empty() {
        return Err(DlqSubjectError::EmptySubject);
    }

    // Check total length
    if subject.len() > 1024 {
        return Err(DlqSubjectError::SubjectTooLong {
            length: subject.len(),
            max: 1024,
        });
    }

    let mut token_start = 0;
    let mut prev_was_dot = false;

    for (i, c) in subject.char_indices() {
        if c == '.' {
            // Check for empty token (consecutive dots or leading/trailing dot)
            if prev_was_dot || i == 0 {
                return Err(DlqSubjectError::EmptyToken { position: i });
            }
            prev_was_dot = true;

            // Check token length (max 255 bytes)
            let token_len = i - token_start;
            if token_len > 255 {
                return Err(DlqSubjectError::TokenTooLong {
                    length: token_len,
                    max: 255,
                    position: token_start,
                });
            }

            token_start = i + 1;
        } else if c.is_whitespace() || c == '\0' || c == '*' || c == '>' || c == '\\' {
            return Err(DlqSubjectError::InvalidCharacters {
                invalid_char: c,
                position: i,
            });
        } else {
            prev_was_dot = false;
        }
    }

    // Check trailing dot
    if subject.ends_with('.') {
        return Err(DlqSubjectError::EmptyToken {
            position: subject.len() - 1,
        });
    }

    // Check final token length
    let final_token_len = subject.len() - token_start;
    if final_token_len > 255 {
        return Err(DlqSubjectError::TokenTooLong {
            length: final_token_len,
            max: 255,
            position: token_start,
        });
    }

    Ok(())
}

// =============================================================================
// DLQ Header Constants
// =============================================================================

/// DLQ subject suffix appended to original subject
const DLQ_SUFFIX: &str = ".DLQ";

/// Header name for original subject (preserved when message is routed to DLQ)
pub const HEADER_ORIG_SUBJECT: &str = "Nats-Orig-Subject";

/// Header name for delivery attempt count when message was DLQ'd
pub const HEADER_DELIVERY_COUNT: &str = "Nats-Deliver-Count";

/// Header name for reason message was sent to DLQ
pub const HEADER_DLQ_REASON: &str = "Nats-DLQ-Reason";

/// Header name for timestamp when message was sent to DLQ
pub const HEADER_DLQ_TIMESTAMP: &str = "Nats-DLQ-Timestamp";

/// Bounded app-level DLQ helper for NATS JetStream messages.
///
/// Provides explicit DLQ subject derivation and message routing primitives.
/// This is a first-slice implementation — not full production DLQ worker.
///
/// **Design:** Per `docs/10-delivery/14-dlq-retry-design.md`:
/// - DLQ subject format: `{origin_subject}.DLQ`
/// - Example: `audit.events.v1.approval.events` → `audit.events.v1.approval.events.DLQ`
#[derive(Debug, Clone)]
pub struct DlqHelper {
    /// JetStream context for publishing
    jetstream: JetStreamContext,
}

impl DlqHelper {
    /// Create a new DLQ helper with the given JetStream context.
    pub fn new(jetstream: JetStreamContext) -> Self {
        Self { jetstream }
    }

    /// Derive DLQ subject from original subject safely.
    ///
    /// **Subject transformation:**
    /// - `audit.events.v1.approval.events` → `audit.events.v1.approval.events.DLQ`
    /// - `audit.events.v1.intent.events` → `audit.events.v1.intent.events.DLQ`
    ///
    /// **Safety constraints:**
    /// - Empty subject returns empty (caller handles error)
    /// - Subject already ending in `.DLQ` is returned as-is (no double-suffix)
    /// - Subject with valid NATS characters is preserved as-is
    ///
    /// **NATS subject rules enforced:**
    /// - Non-empty tokens separated by dots
    /// - No whitespace, null bytes, or special NATS metacharacters
    /// - Max token length: 255 bytes
    /// - Max subject length: 1024 bytes (NATS protocol limit)
    pub fn derive_dlq_subject(original_subject: &str) -> Result<String, DlqSubjectError> {
        // Handle empty subject
        if original_subject.is_empty() {
            return Err(DlqSubjectError::EmptySubject);
        }

        // Check for already-DLQ'd subject
        if original_subject.ends_with(DLQ_SUFFIX) {
            return Ok(original_subject.to_string());
        }

        // Validate subject length (NATS protocol limit is 1024 bytes)
        if original_subject.len() > 1024 {
            return Err(DlqSubjectError::SubjectTooLong {
                length: original_subject.len(),
                max: 1024,
            });
        }

        // Validate NATS subject syntax
        validate_nats_subject(original_subject)?;

        // Append DLQ suffix
        Ok(format!("{}{}", original_subject, DLQ_SUFFIX))
    }

    /// Publish a failed message to the DLQ subject.
    ///
    /// **Behavior:**
    /// - Derives DLQ subject from original message subject
    /// - Copies payload to DLQ message
    /// - Adds DLQ metadata headers (orig subject, delivery count, reason, timestamp)
    /// - Emits `intent_api_dlq_messages_total` metric via lib.rs helper
    ///
    /// **Headers added:**
    /// - `Nats-Orig-Subject`: Original subject before DLQ routing
    /// - `Nats-Deliver-Count`: Delivery attempt count when message was DLQ'd
    /// - `Nats-DLQ-Reason`: Human-readable reason for DLQ (e.g., "max_deliver_exceeded", "consumer_failed")
    /// - `Nats-DLQ-Timestamp`: RFC3339 timestamp when message was sent to DLQ
    ///
    /// **Returns:** `Ok(())` if publish succeeded, `Err` if publish failed.
    ///
    /// **Metric emitted:** `intent_api_dlq_messages_total` (via `record_dlq_message()` in lib.rs)
    pub async fn publish_to_dlq(
        &self,
        original_subject: &str,
        payload: Vec<u8>,
        delivery_count: u64,
        reason: &str,
    ) -> Result<(), DlqPublishError> {
        // Derive DLQ subject
        let dlq_subject = Self::derive_dlq_subject(original_subject)
            .map_err(|e| DlqPublishError::SubjectDerivation(e.to_string()))?;

        // Build DLQ headers
        let mut headers = async_nats::HeaderMap::new();
        headers.insert(HEADER_ORIG_SUBJECT, original_subject);
        headers.insert(HEADER_DELIVERY_COUNT, delivery_count.to_string());
        headers.insert(HEADER_DLQ_REASON, reason);
        headers.insert(HEADER_DLQ_TIMESTAMP, chrono::Utc::now().to_rfc3339());

        // Publish to DLQ subject - payload converted to Bytes via Into trait
        self.jetstream
            .publish_with_headers(dlq_subject.clone(), headers, payload.into())
            .await
            .map_err(|e| DlqPublishError::PublishFailed(e.to_string()))?;

        // Emit DLQ metric via lib.rs helper
        crate::record_dlq_message();

        tracing::debug!(
            "DlqHelper: published message to DLQ subject '{}', reason: {}, delivery_count: {}",
            dlq_subject,
            reason,
            delivery_count
        );

        Ok(())
    }

    /// Replay a message from DLQ back to its original subject.
    ///
    /// **Behavior:**
    /// - Extracts original subject from `Nats-Orig-Subject` header
    /// - Publishes payload to original subject with replay header
    /// - Emits `intent_api_dlq_replay_total` metric with status label
    ///
    /// **Headers preserved:**
    /// - Original traceparent (if present)
    /// - Any user-defined headers except DLQ-specific ones
    ///
    /// **Headers added:**
    /// - `Nats-Replay`: Set to `"true"` to indicate replay from DLQ
    ///
    /// **Returns:** `Ok(())` if replay succeeded, `Err` if replay failed.
    ///
    /// **Metric emitted:** `intent_api_dlq_replay_total{status="success|error"}`
    pub async fn replay_from_dlq(
        &self,
        dlq_message: &async_nats::jetstream::Message,
    ) -> Result<(), DlqReplayError> {
        // Extract original subject from header
        let original_subject = dlq_message
            .headers
            .as_ref()
            .and_then(|h| h.get(HEADER_ORIG_SUBJECT))
            .ok_or(DlqReplayError::MissingOrigSubjectHeader)?
            .to_string();

        // Extract payload
        let payload = dlq_message.payload.to_vec();

        // Build replay headers:
        // - Nats-Replay marker to indicate this is a replay
        // - Original subject preserved for traceability
        // - Original traceparent preserved if present (for distributed tracing continuity)
        // Note: DLQ-specific headers are not copied to replay (they served their purpose)
        let mut replay_headers = async_nats::HeaderMap::new();
        replay_headers.insert("Nats-Replay", "true");
        replay_headers.insert(HEADER_ORIG_SUBJECT, original_subject.as_str());

        // Preserve traceparent header if present for distributed tracing continuity
        if let Some(traceparent) = dlq_message
            .headers
            .as_ref()
            .and_then(|h| h.get("traceparent"))
        {
            replay_headers.insert("traceparent", traceparent.as_str());
        }

        // Publish to original subject - payload is already Vec<u8>, convert via Into
        match self
            .jetstream
            .publish_with_headers(original_subject.clone(), replay_headers, payload.into())
            .await
        {
            Ok(_ack) => {
                // Emit success metric via lib.rs helper
                crate::record_dlq_replay("success");
                tracing::debug!(
                    "DlqHelper: replayed message from DLQ to original subject '{}'",
                    original_subject
                );
                Ok(())
            }
            Err(e) => {
                // Emit failure metric via lib.rs helper
                crate::record_dlq_replay_failure();
                Err(DlqReplayError::PublishFailed(e.to_string()))
            }
        }
    }

    /// Replay a message from DLQ to a specific target subject.
    ///
    /// Unlike `replay_from_dlq`, this allows replaying to a different subject
    /// than the original (useful for testing or directed replay).
    ///
    /// **Returns:** `Ok(())` if publish succeeded, `Err` if publish failed.
    pub async fn replay_to_subject(
        &self,
        original_subject: &str,
        payload: Vec<u8>,
        target_subject: &str,
    ) -> Result<(), DlqReplayError> {
        // Validate target subject
        validate_nats_subject(target_subject)
            .map_err(|e| DlqReplayError::InvalidSubject(e.to_string()))?;

        // Convert to owned strings for publish method ownership requirement
        let target_owned = target_subject.to_string();
        let orig_owned = original_subject.to_string();

        // Build replay headers with owned strings
        let mut replay_headers = async_nats::HeaderMap::new();
        replay_headers.insert("Nats-Replay", "true");
        replay_headers.insert(HEADER_ORIG_SUBJECT, orig_owned.as_str());

        // Publish to target subject - payload converted to Bytes via Into trait
        match self
            .jetstream
            .publish_with_headers(target_owned, replay_headers, payload.into())
            .await
        {
            Ok(_ack) => {
                crate::record_dlq_replay("success");
                tracing::debug!(
                    "DlqHelper: replayed message from '{}' to target subject '{}'",
                    original_subject,
                    target_subject
                );
                Ok(())
            }
            Err(e) => {
                crate::record_dlq_replay_failure();
                Err(DlqReplayError::PublishFailed(e.to_string()))
            }
        }
    }
}
