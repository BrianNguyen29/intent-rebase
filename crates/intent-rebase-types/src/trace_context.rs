//! Trace context propagation for bounded audit/event continuity slice
//!
//! ## Phase 3 Bounded Trace Continuity Slice
//!
//! This module provides trace context (trace_id, span_id) capture for audit events
//! and published event envelopes. This is a **bounded in-process slice**:
//!
//! **What IS implemented:**
//! - Capture active trace context when recording audit events
//! - Carry trace_id/span_id in `EventEnvelope` and `PublishedEvent`
//! - Database columns already exist (`trace_id`, `span_id` in `audit_events` table)
//! - Helper to extract trace context from current tracing span via global tracer
//!
//! **What is NOT implemented (future scope):**
//! - Cross-process trace propagation via Temporal gRPC headers
//! - Cross-process propagation via sqlx connection context
//! - W3C trace-context injection into outbound NATS messages
//! - Full OTEL span lifecycle management
//!
//! ## Design Decisions
//!
//! - Uses `opentelemetry::global::tracer()` to get current span context without
//!   requiring `tracing-opentelemetry` as a direct dependency (keeps intent-rebase-types lightweight)
//! - Trace context is OPTIONAL on all APIs — callers pass it explicitly when available
//! - When no active span exists, helper returns None for both fields
//! - The global tracer approach works regardless of whether OTLP export is configured

use opentelemetry::trace::{SpanContext, TraceContextExt};

/// Bounded trace context for audit/event continuity.
///
/// This struct carries the minimum required fields (trace_id, span_id)
/// for correlating audit events and published envelopes with the active trace.
///
/// **Bounded scope:** This is for in-process trace context capture only.
/// Cross-process propagation (Temporal, sqlx, NATS) is future scope.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TraceContext {
    /// W3C trace_id (32 hex chars, lowercase)
    pub trace_id: Option<String>,
    /// W3C span_id (16 hex chars, lowercase)
    pub span_id: Option<String>,
}

impl TraceContext {
    /// Create a new TraceContext with the given values
    pub fn new(trace_id: Option<String>, span_id: Option<String>) -> Self {
        Self { trace_id, span_id }
    }

    /// Returns true if both trace_id and span_id are present
    pub fn is_some(&self) -> bool {
        self.trace_id.is_some() && self.span_id.is_some()
    }

    /// Returns true if neither trace_id nor span_id are present
    pub fn is_none(&self) -> bool {
        self.trace_id.is_none() && self.span_id.is_none()
    }
}

/// Try to extract trace context from the current active span using the global tracer.
///
/// This function retrieves the current active span from the global tracer provider,
/// if any. It works regardless of whether OTLP export is configured.
///
/// Returns `TraceContext` with both fields `None` if:
/// - No global tracer provider is set
/// - No active span exists
/// - The span context is invalid
///
/// **Bounded scope note:** This captures in-process trace context only.
/// Cross-process propagation (Temporal headers, sqlx context, NATS headers) is future scope.
pub fn get_current_trace_context() -> TraceContext {
    // Get the current active span from the OpenTelemetry context
    let context = opentelemetry::Context::current();
    let span = context.span();

    let span_context = span.span_context();

    if !span_context.is_valid() {
        return TraceContext::default();
    }

    // Convert to hex strings (W3C standard format)
    let trace_id_hex = format!("{:032x}", span_context.trace_id());
    let span_id_hex = format!("{:016x}", span_context.span_id());

    TraceContext {
        trace_id: Some(trace_id_hex),
        span_id: Some(span_id_hex),
    }
}

/// Extract trace context from a SpanContext directly.
///
/// Useful when you already have a span context from interop with other code.
pub fn from_span_context(span_context: &SpanContext) -> TraceContext {
    if !span_context.is_valid() {
        return TraceContext::default();
    }

    TraceContext {
        trace_id: Some(format!("{:032x}", span_context.trace_id())),
        span_id: Some(format!("{:016x}", span_context.span_id())),
    }
}

// =============================================================================
// W3C Traceparent Parsing (Phase 3 bounded trace continuity slice)
// =============================================================================

/// Error type for malformed traceparent headers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TraceparentError {
    /// traceparent header value is empty
    Empty,
    /// Invalid format (wrong number of components)
    InvalidFormat,
    /// Invalid version field (not "00")
    InvalidVersion,
    /// trace_id is not valid hex or wrong length (must be 32 chars)
    InvalidTraceId,
    /// span_id is not valid hex or wrong length (must be 16 chars)
    InvalidSpanId,
    /// trace_flags is not valid hex or wrong length (must be 2 chars)
    InvalidTraceFlags,
}

/// W3C traceparent header parser.
///
/// Format: `version-trace_id-span_id-trace_flags`
/// Example: `00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01`
///
/// Phase 3 bounded trace continuity slice: Parses native NATS `traceparent` header
/// into `TraceContext` for correlation. Rejects malformed headers.
///
/// **Validation rules:**
/// - Version must be "00" (only version supported)
/// - trace_id must be exactly 32 hex characters (0-9, a-f, A-F)
/// - span_id must be exactly 16 hex characters (0-9, a-f, A-F)
/// - trace_flags must be exactly 2 hex characters (0-9, a-f, A-F)
/// - Per W3C spec, implementations SHOULD accept uppercase hex digits (we accept both)
pub fn parse_traceparent(value: &str) -> Result<TraceContext, TraceparentError> {
    if value.is_empty() {
        return Err(TraceparentError::Empty);
    }

    let parts: Vec<&str> = value.split('-').collect();
    if parts.len() != 4 {
        return Err(TraceparentError::InvalidFormat);
    }

    let [version, trace_id, span_id, trace_flags] = parts.as_slice() else {
        return Err(TraceparentError::InvalidFormat);
    };

    // Version must be "00" (W3C trace-context version)
    if *version != "00" {
        return Err(TraceparentError::InvalidVersion);
    }

    // trace_id: exactly 32 hex characters (W3C hexdig: DIGIT / "a" / "f")
    // Per W3C spec: implementations SHOULD accept uppercase and convert to lowercase.
    // For this bounded Phase 3 implementation, we accept valid hex digits (case-insensitive).
    if trace_id.len() != 32 || !trace_id.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(TraceparentError::InvalidTraceId);
    }

    // span_id: exactly 16 hex characters
    if span_id.len() != 16 || !span_id.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(TraceparentError::InvalidSpanId);
    }

    // trace_flags: exactly 2 hex characters
    if trace_flags.len() != 2 || !trace_flags.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(TraceparentError::InvalidTraceFlags);
    }

    Ok(TraceContext {
        trace_id: Some(trace_id.to_string()),
        span_id: Some(span_id.to_string()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trace_context_default() {
        let ctx = TraceContext::default();
        assert!(ctx.is_none());
        assert_eq!(ctx.trace_id, None);
        assert_eq!(ctx.span_id, None);
    }

    #[test]
    fn test_trace_context_new() {
        let ctx = TraceContext::new(Some("abc123".to_string()), Some("def456".to_string()));
        assert!(ctx.is_some());
        assert_eq!(ctx.trace_id, Some("abc123".to_string()));
        assert_eq!(ctx.span_id, Some("def456".to_string()));
    }

    #[test]
    fn test_trace_context_serialization() {
        let ctx = TraceContext::new(
            Some("0af7651916cd43dd8448eb211c80319c".to_string()),
            Some("b7ad6b7169203331".to_string()),
        );
        let json = serde_json::to_string(&ctx).unwrap();
        assert!(json.contains("0af7651916cd43dd8448eb211c80319c"));
        assert!(json.contains("b7ad6b7169203331"));
    }

    #[test]
    fn test_get_current_trace_context_no_tracer() {
        // When no global tracer is set, should return default
        let ctx = get_current_trace_context();
        // This may or may not be None depending on whether other code has initialized the global
        // The important thing is it doesn't panic
        println!("Current trace context: {:?}", ctx);
    }

    // =====================================================================
    // W3C Traceparent Parsing Tests (Phase 3 bounded trace continuity slice)
    // =====================================================================

    #[test]
    fn test_parse_traceparent_valid() {
        let result = parse_traceparent("00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01");
        assert!(result.is_ok());
        let ctx = result.unwrap();
        assert_eq!(
            ctx.trace_id,
            Some("0af7651916cd43dd8448eb211c80319c".to_string())
        );
        assert_eq!(ctx.span_id, Some("b7ad6b7169203331".to_string()));
    }

    #[test]
    fn test_parse_traceparent_valid_unsampled() {
        // trace_flags "00" means not sampled
        let result = parse_traceparent("00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-00");
        assert!(result.is_ok());
        let ctx = result.unwrap();
        assert_eq!(
            ctx.trace_id,
            Some("0af7651916cd43dd8448eb211c80319c".to_string())
        );
        assert_eq!(ctx.span_id, Some("b7ad6b7169203331".to_string()));
    }

    #[test]
    fn test_parse_traceparent_empty() {
        let result = parse_traceparent("");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), TraceparentError::Empty);
    }

    #[test]
    fn test_parse_traceparent_invalid_format_too_few_parts() {
        let result = parse_traceparent("00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), TraceparentError::InvalidFormat);
    }

    #[test]
    fn test_parse_traceparent_invalid_format_too_many_parts() {
        let result =
            parse_traceparent("00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01-extra");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), TraceparentError::InvalidFormat);
    }

    #[test]
    fn test_parse_traceparent_invalid_version() {
        // Version "FF" is not supported
        let result = parse_traceparent("FF-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), TraceparentError::InvalidVersion);
    }

    #[test]
    fn test_parse_traceparent_trace_id_too_short() {
        // 31 chars instead of 32
        let result = parse_traceparent("00-0af7651916cd43dd8448eb211c80319-b7ad6b7169203331-01");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), TraceparentError::InvalidTraceId);
    }

    #[test]
    fn test_parse_traceparent_trace_id_invalid_hex() {
        // Contains 'g' which is not a hex character
        let result = parse_traceparent("00-0af7651916cd43dd8448eb211c80319g-b7ad6b7169203331-01");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), TraceparentError::InvalidTraceId);
    }

    #[test]
    fn test_parse_traceparent_trace_id_uppercase() {
        // W3C spec says we SHOULD accept uppercase hex digits and convert to lowercase.
        // For this bounded implementation, we accept uppercase as valid hex.
        let result = parse_traceparent("00-0AF7651916CD43DD8448EB211C80319C-b7ad6b7169203331-01");
        assert!(result.is_ok());
        let ctx = result.unwrap();
        // Note: we accept uppercase but don't convert it
        assert_eq!(
            ctx.trace_id,
            Some("0AF7651916CD43DD8448EB211C80319C".to_string())
        );
    }

    #[test]
    fn test_parse_traceparent_span_id_too_short() {
        // 15 chars instead of 16
        let result = parse_traceparent("00-0af7651916cd43dd8448eb211c80319c-b7ad6b716920333-01");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), TraceparentError::InvalidSpanId);
    }

    #[test]
    fn test_parse_traceparent_span_id_invalid_hex() {
        // Contains 'z' which is not a hex character
        let result = parse_traceparent("00-0af7651916cd43dd8448eb211c80319c-b7ad6b716920333z-01");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), TraceparentError::InvalidSpanId);
    }

    #[test]
    fn test_parse_traceparent_span_id_uppercase() {
        // W3C spec says we SHOULD accept uppercase hex digits.
        // For this bounded implementation, we accept uppercase as valid hex.
        let result = parse_traceparent("00-0af7651916cd43dd8448eb211c80319c-B7AD6B7169203331-01");
        assert!(result.is_ok());
        let ctx = result.unwrap();
        assert_eq!(ctx.span_id, Some("B7AD6B7169203331".to_string()));
    }

    #[test]
    fn test_parse_traceparent_flags_too_short() {
        // 1 char instead of 2
        let result = parse_traceparent("00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-1");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), TraceparentError::InvalidTraceFlags);
    }

    #[test]
    fn test_parse_traceparent_flags_invalid_hex() {
        // Contains 'z' which is not a hex character
        let result = parse_traceparent("00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-g1");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), TraceparentError::InvalidTraceFlags);
    }

    #[test]
    fn test_parse_traceparent_flags_uppercase() {
        // W3C spec says we SHOULD accept uppercase hex digits.
        // For this bounded implementation, we accept uppercase as valid hex.
        let result = parse_traceparent("00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-A1");
        // Note: We don't validate trace_flags format beyond being valid hex (01 is sampled, 00 is not)
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_traceparent_all_zeros() {
        // Valid: all zeros trace_id and span_id
        let result = parse_traceparent("00-00000000000000000000000000000000-0000000000000000-01");
        assert!(result.is_ok());
        let ctx = result.unwrap();
        assert_eq!(
            ctx.trace_id,
            Some("00000000000000000000000000000000".to_string())
        );
        assert_eq!(ctx.span_id, Some("0000000000000000".to_string()));
    }

    #[test]
    fn test_parse_traceparent_roundtrip() {
        // Parse a traceparent, verify we get the expected TraceContext
        let original = "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01";
        let ctx = parse_traceparent(original).unwrap();
        assert!(ctx.is_some());
        assert_eq!(
            ctx.trace_id.as_ref().unwrap(),
            "0af7651916cd43dd8448eb211c80319c"
        );
        assert_eq!(ctx.span_id.as_ref().unwrap(), "b7ad6b7169203331");
    }
}
