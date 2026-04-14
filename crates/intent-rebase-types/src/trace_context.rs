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
}
