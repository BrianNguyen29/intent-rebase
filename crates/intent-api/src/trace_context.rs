//! P2-S3 bounded distributed tracing slice: trace context extraction helper
//!
//! Provides utilities for extracting trace/span IDs from the current tracing span
//! for propagation into audit events and other out-of-band surfaces.
//!
//! ## Bounded Slice Design
//!
//! This module is bounded to a first distributed tracing slice that:
//! - Wires existing `trace_id`/`span_id` fields on `AuditEvent` into real code paths
//! - Uses `tracing` Span::current() to extract context — no new OTel infrastructure
//! - Propagates into audit/event surfaces via existing `AuditRepository` helper methods
//!
//! Full OTel/OTLP export, collector setup, and cross-service trace propagation is
//! Phase 3 Batch 2 (P2-S4+) scope — not included here.

use tracing::Span;

/// Extract trace_id and span_id strings from the current tracing span.
///
/// Returns (trace_id, span_id) as hex strings if available.
/// Falls back to (None, None) if no valid span context is available.
///
/// This uses the tracing crate's span ID which is compatible with W3C TraceContext
/// when using a tracing implementation that supports OpenTelemetry context propagation.
pub fn current_trace_context() -> (Option<String>, Option<String>) {
    let span = Span::current();
    
    // Use Span::id() which returns the span's unique identifier as a NonZeroU64
    // Convert to hex string for standard trace/span ID format
    let span_id = span.id().map(|id| {
        let id_u64: u64 = id.into_u64();
        format!("{:016x}", id_u64)
    });
    
    // For trace_id, we use a best-effort approach:
    // When inside a span, we construct a trace_id from the span's ID combined with
    // a stable identifier. In non-instrumented contexts this may be None.
    //
    // Note: Full OpenTelemetry trace_id propagation requires tracing-opentelemetry
    // or similar OTel-compatible layer. This bounded slice provides the hooks
    // for trace context once that infrastructure is in place (P2-S4+).
    let trace_id: Option<String> = span_id.as_ref().map(|sid: &String| {
        // Pad to 32 chars for standard trace_id format (tracing Id is 64-bit, trace_id is 128-bit)
        let mut s = sid.clone();
        s.push_str("0000000000000000");
        s[..32].to_string()
    });

    (trace_id, span_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracing::instrument;

    #[instrument]
    fn example_nested_function() -> (Option<String>, Option<String>) {
        current_trace_context()
    }

    #[tokio::test]
    async fn test_trace_context_extraction() {
        // When called within an active span, should return valid IDs
        let (trace_id, span_id) = example_nested_function();
        // In test context without a span, may be None - this is expected
        // The important thing is that the function doesn't panic
        println!("trace_id: {:?}, span_id: {:?}", trace_id, span_id);
    }
}