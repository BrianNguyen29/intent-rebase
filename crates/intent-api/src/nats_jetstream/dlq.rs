//! DLQ error types and NATS subject validation.
//!
//! Bounded helper decomposition slice (S2) from `nats_jetstream.rs`.
//! Contains error types for DLQ subject derivation, publishing, and replay,
//! plus `validate_nats_subject` for checking NATS subject compliance.

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
                    super::HEADER_ORIG_SUBJECT
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
